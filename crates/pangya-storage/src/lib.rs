#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

//! PostgreSQL 17 pool, migrations, and M2 account/handover repositories.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    future::Future,
    sync::Arc,
    time::{Duration, SystemTime},
};

use chrono::{DateTime, Utc};
use pangya_domain::{
    AbortMatch, AbortMatchOutcome, AbortStrokeMatch, AbortStrokeMatchOutcome, Account,
    AccountAggregate, AccountBalances, AccountId, AccountRepository, AccountStatus,
    AuthenticatedSession, AuthenticationRecord, BalanceGrant, BeginSoloMatch,
    BeginSoloMatchOutcome, BeginStrokeMatch, BeginStrokeMatchOutcome, Character, CharacterId,
    CommitSoloHole, CommitStrokeMatch, ConsumeHandover, ConsumeItem, ConsumeItemResult,
    CredentialHash, EconomyCommit, EconomyError, EconomyOperationId, EconomyRepository,
    EquipmentChange, EquipmentChangeResult, EquipmentSet, EquipmentSetId, HandoverDigest,
    HandoverError, HandoverRepository, IncompleteMatchAbortLimit, InventoryClass,
    InventoryDurability, InventoryItem, InventoryItemId, ItemDurability, ItemKind, ItemSale,
    ItemStacking, ItemTypeId, LoginBonusClaim, LoginBonusReward, MAX_PLAYER_CHARACTERS,
    MAX_PLAYER_INVENTORY, MAX_RECENT_PLAYERS, MAX_STARTER_ITEMS, MarkSoloInGame,
    MarkSoloInGameOutcome, MarkStrokeInGame, MarkStrokeInGameOutcome, MascotMessageUpdate,
    MatchAbortReason, MatchId, MatchRepository, MatchRepositoryError, MatchResultKey,
    MessageEligibilityRepository, MyRoomFurniture, MyRoomProjection, NewAccount, NewHandover,
    NewMessageEligibility, Nickname,
    NoopStorageObserver, NormalizedNickname, NormalizedUsername, OfflineNote, OfflineNoteClaim,
    OfflineNoteCommit, OfflineNoteRequest, PlayerRepository, PlayerSnapshot, Profile,
    PurchaseRequest, PurchaseResult, RecentPlayer, RepairItem, RepairItemResult, RepositoryError,
    RepositoryFuture, RetailEquipmentChange, RetailEquipmentState, ServerBalances, ServiceKind,
    SetupState, SoloMatchResult, StarterGrant, StarterKey, StorageFault, StorageFaulted,
    StorageObserver, StrokeCompletion, StrokeCount, StrokeMatchResult, StrokePlace,
    StrokePlayerCommit, StrokePlayerResult, StrokeReward, StrokeRosterOrder, Weather,
    WindConditions, synthetic_solo_reward_v1, synthetic_stroke_reward_v1,
};
use sqlx::{
    FromRow, PgPool, Postgres, Row, Transaction,
    migrate::Migrator,
    postgres::{PgConnectOptions, PgConnection, PgPoolOptions},
};
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

mod admin;
mod economy;

/// Embedded forward-only PostgreSQL migrations.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Redacted PostgreSQL pool configuration.
#[derive(Clone)]
pub struct PgStorageConfig {
    database_url: String,
    /// Maximum open connections.
    pub max_connections: u32,
    /// Minimum retained connections.
    pub min_connections: u32,
    /// Maximum wait to acquire a connection.
    pub acquire_timeout: Duration,
    /// Maximum idle duration before a connection is closed.
    pub idle_timeout: Option<Duration>,
    /// Maximum connection lifetime.
    pub max_lifetime: Option<Duration>,
}

impl PgStorageConfig {
    /// Creates a bounded pool configuration around a secret database URL.
    #[must_use]
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            max_connections: 10,
            min_connections: 1,
            acquire_timeout: Duration::from_secs(5),
            idle_timeout: Some(Duration::from_secs(600)),
            max_lifetime: Some(Duration::from_secs(1_800)),
        }
    }

    /// Builds and connects the configured PostgreSQL pool.
    ///
    /// # Errors
    /// Returns a redacted configuration/connect error.
    pub async fn connect(&self) -> Result<PgPool, StorageBootstrapError> {
        if self.max_connections == 0
            || self.max_connections > 256
            || self.min_connections > self.max_connections
            || self.acquire_timeout.is_zero()
            || self.acquire_timeout > Duration::from_secs(60)
        {
            return Err(StorageBootstrapError::InvalidConfig);
        }
        let options: PgConnectOptions = self
            .database_url
            .parse()
            .map_err(|_| StorageBootstrapError::InvalidConfig)?;
        PgPoolOptions::new()
            .max_connections(self.max_connections)
            .min_connections(self.min_connections)
            .acquire_timeout(self.acquire_timeout)
            .idle_timeout(self.idle_timeout)
            .max_lifetime(self.max_lifetime)
            .connect_with(options)
            .await
            .map_err(|_| StorageBootstrapError::Connect)
    }
}

impl fmt::Debug for PgStorageConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PgStorageConfig")
            .field("database_url", &"[REDACTED]")
            .field("max_connections", &self.max_connections)
            .field("min_connections", &self.min_connections)
            .field("acquire_timeout", &self.acquire_timeout)
            .field("idle_timeout", &self.idle_timeout)
            .field("max_lifetime", &self.max_lifetime)
            .finish()
    }
}

/// Pool/migration bootstrap error with connection secrets removed.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum StorageBootstrapError {
    /// Pool bounds or URL syntax were invalid.
    #[error("PostgreSQL storage configuration is invalid")]
    InvalidConfig,
    /// Pool connection failed.
    #[error("PostgreSQL connection failed")]
    Connect,
    /// Embedded migration failed.
    #[error("PostgreSQL migration failed")]
    Migration,
}

/// Runs all embedded forward migrations.
///
/// # Errors
/// Returns a redacted migration failure.
pub async fn migrate(pool: &PgPool) -> Result<(), StorageBootstrapError> {
    MIGRATOR
        .run(pool)
        .await
        .map_err(|_| StorageBootstrapError::Migration)
}

/// PostgreSQL implementation of the M2 repository contracts.
#[derive(Clone)]
pub struct PgRepository {
    pool: PgPool,
    observer: Arc<dyn StorageObserver>,
}

impl PgRepository {
    /// Wraps a configured pool and discards fault observations.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            observer: Arc::new(NoopStorageObserver),
        }
    }

    /// Wraps a configured pool and reports every classified fault to `observer`.
    #[must_use]
    pub fn with_observer(pool: PgPool, observer: Arc<dyn StorageObserver>) -> Self {
        Self { pool, observer }
    }

    /// Reports the classified fault an inner call produced, then returns it unchanged.
    ///
    /// Observation is the only side effect: the result is passed through untouched, so
    /// control flow and every caller-visible outcome are identical with and without an
    /// observer installed.
    async fn observed<T, E, F>(&self, inner: F) -> Result<T, E>
    where
        E: StorageFaulted,
        F: Future<Output = Result<T, E>>,
    {
        let result = inner.await;
        if let Err(error) = &result
            && let Some(fault) = error.storage_fault()
        {
            self.observer.storage_fault(fault);
        }
        result
    }

    /// Atomically creates an operator account aggregate and its success audit event.
    ///
    /// # Errors
    /// Returns a friendly repository error; an audit failure rolls back the entire aggregate.
    pub async fn create_operator_account(
        &self,
        request: NewAccount,
    ) -> Result<AccountAggregate, RepositoryError> {
        self.observed(self.create_account_inner(request, true))
            .await
    }

    /// Records a durable, nonsecret operator audit event after DB availability.
    ///
    /// # Errors
    /// Returns a redacted storage failure.
    pub async fn record_operator_audit(
        &self,
        action: &'static str,
        account_id: Option<AccountId>,
        outcome: &'static str,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            "INSERT INTO operator_audit_events (action, account_id, outcome) \
             VALUES ($1, $2, $3)",
            action,
            account_id.map(AccountId::get),
            outcome
        )
        .execute(&self.pool)
        .await
        .map_err(|error| {
            let mapped = repository_db_error(error);
            if let Some(fault) = mapped.storage_fault() {
                self.observer.storage_fault(fault);
            }
            mapped
        })?;
        Ok(())
    }

    /// Returns the pool for readiness and transaction composition, never SQL rows.
    #[must_use]
    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn create_account_inner(
        &self,
        request: NewAccount,
        audit_success: bool,
    ) -> Result<AccountAggregate, RepositoryError> {
        ensure_starter_bounds(&request.starter)?;
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let account_id: i64 = sqlx::query_scalar!(
            "INSERT INTO accounts (username_normalized, username_display) \
             VALUES ($1, $2) RETURNING id",
            request.username.normalized().as_str(),
            request.username.display()
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let account_id = AccountId::new(account_id).map_err(|_| RepositoryError::CorruptData)?;

        sqlx::query!(
            "INSERT INTO credentials (account_id, scheme, password_hash) \
             VALUES ($1, 'argon2id-client-md5-v1', $2)",
            account_id.get(),
            request.credential_hash.expose_phc()
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;

        let (nickname_display, nickname_normalized, setup_state) = match &request.nickname {
            Some(nickname) => (
                Some(nickname.display()),
                Some(nickname.normalized().as_str()),
                "needs_starter",
            ),
            None => (None, None, "needs_nickname"),
        };
        sqlx::query!(
            "INSERT INTO profiles \
             (account_id, nickname_display, nickname_normalized, setup_state) \
             VALUES ($1, $2, $3, $4)",
            account_id.get(),
            nickname_display,
            nickname_normalized,
            setup_state
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;

        apply_starter(&mut transaction, account_id, &request.starter).await?;
        if audit_success {
            sqlx::query!(
                "INSERT INTO operator_audit_events (action, account_id, outcome) \
                 VALUES ('account_create', $1, 'success')",
                account_id.get()
            )
            .execute(&mut *transaction)
            .await
            .map_err(repository_db_error)?;
        }
        let aggregate = load_aggregate_in_transaction(&mut transaction, account_id).await?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(aggregate)
    }

    async fn load_authentication_inner(
        &self,
        username: &NormalizedUsername,
    ) -> Result<Option<AuthenticationRecord>, RepositoryError> {
        let row = sqlx::query_as::<_, AuthenticationRow>(
            "SELECT a.id, a.username_display, a.username_normalized, a.status, \
                    c.password_hash, p.setup_state, p.nickname_display \
             FROM accounts a \
             JOIN credentials c ON c.account_id = a.id \
             JOIN profiles p ON p.account_id = a.id \
             WHERE a.username_normalized = $1",
        )
        .bind(username.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_db_error)?;
        row.map(AuthenticationRow::into_domain).transpose()
    }

    async fn set_nickname_inner(
        &self,
        account_id: AccountId,
        nickname: Nickname,
    ) -> Result<(), RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        lock_active_account(&mut transaction, account_id).await?;
        let result = sqlx::query!(
            "UPDATE profiles SET nickname_display = $2, nickname_normalized = $3, \
                    setup_state = CASE WHEN setup_state = 'needs_nickname' \
                        THEN 'complete' ELSE setup_state END, updated_at = now() \
             WHERE account_id = $1",
            account_id.get(),
            nickname.display(),
            nickname.normalized().as_str()
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(())
    }

    async fn nickname_available_inner(
        &self,
        nickname: &NormalizedNickname,
    ) -> Result<bool, RepositoryError> {
        sqlx::query_scalar!(
            r#"SELECT NOT EXISTS(
                SELECT 1 FROM profiles WHERE nickname_normalized = $1
            ) AS "available!""#,
            nickname.as_str()
        )
        .fetch_one(&self.pool)
        .await
        .map_err(repository_db_error)
    }

    async fn grant_starter_inner(
        &self,
        account_id: AccountId,
        grant: StarterGrant,
    ) -> Result<AccountAggregate, RepositoryError> {
        ensure_starter_bounds(&grant)?;
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        apply_starter(&mut transaction, account_id, &grant).await?;
        let aggregate = load_aggregate_in_transaction(&mut transaction, account_id).await?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(aggregate)
    }

    /// Repoints the provisional starter character, refusing once setup is complete.
    ///
    /// The same locks as [`apply_starter`] are taken in the same order, so this cannot interleave
    /// with a concurrent grant. The row count is verified rather than assumed: a silent zero-row
    /// update would leave the caller believing the player's choice was stored.
    async fn select_starter_character_inner(
        &self,
        account_id: AccountId,
        item_type_id: ItemTypeId,
    ) -> Result<(), RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        lock_active_account(&mut transaction, account_id).await?;
        let profile = sqlx::query!(
            "SELECT setup_state FROM profiles WHERE account_id = $1 FOR UPDATE",
            account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .ok_or(RepositoryError::NotFound)?;
        if profile.setup_state == "complete" {
            return Err(RepositoryError::InvalidStarterGrant);
        }
        let characters = sqlx::query!(
            "SELECT id, item_type_id FROM characters WHERE account_id = $1 ORDER BY id FOR UPDATE",
            account_id.get()
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let [character] = characters.as_slice() else {
            return Err(RepositoryError::InvalidStarterGrant);
        };
        let requested = i64::from(item_type_id.get());
        if character.item_type_id == requested {
            transaction.commit().await.map_err(repository_db_error)?;
            return Ok(());
        }
        let affected = sqlx::query!(
            "UPDATE characters SET item_type_id = $2 WHERE account_id = $1 AND id = $3",
            account_id.get(),
            requested,
            character.id
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .rows_affected();
        if affected != 1 {
            return Err(RepositoryError::Storage(StorageFault::UnexpectedRowCount));
        }
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(())
    }

    async fn load_chat_macros_inner(
        &self,
        account_id: AccountId,
    ) -> Result<[Vec<u8>; 9], RepositoryError> {
        let row = sqlx::query("SELECT chat_macros FROM profiles WHERE account_id = $1")
            .bind(account_id.get())
            .fetch_optional(&self.pool)
            .await
            .map_err(repository_db_error)?
            .ok_or(RepositoryError::NotFound)?;
        let values: Vec<Vec<u8>> = row.try_get("chat_macros").map_err(repository_db_error)?;
        if values.len() != 9 || values.iter().any(|value| value.len() >= 64) {
            return Err(RepositoryError::CorruptData);
        }
        values.try_into().map_err(|_| RepositoryError::CorruptData)
    }

    async fn save_chat_macros_inner(
        &self,
        account_id: AccountId,
        macros: [Vec<u8>; 9],
    ) -> Result<(), RepositoryError> {
        if macros
            .iter()
            .any(|value| value.len() >= 64 || value.contains(&0))
        {
            return Err(RepositoryError::CorruptData);
        }
        sqlx::query(
            "UPDATE profiles SET chat_macros = $2, updated_at = now() WHERE account_id = $1",
        )
        .bind(account_id.get())
        .bind(macros.to_vec())
        .execute(&self.pool)
        .await
        .map_err(repository_db_error)?;
        Ok(())
    }

    async fn claim_offline_notes_inner(
        &self,
        recipient_id: AccountId,
    ) -> Result<Vec<OfflineNote>, RepositoryError> {
        // Claiming is deliberately separate from delivery acknowledgement. A process crash or
        // socket failure leaves the row leased, not lost; the short lease makes it claimable by a
        // reconnect without charging the sender again.
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let rows = sqlx::query(
            "SELECT n.id, p.nickname_display, n.message \
             FROM offline_notes n \
             JOIN profiles p ON p.account_id = n.sender_account_id \
             WHERE n.recipient_account_id = $1 AND n.delivered_at IS NULL \
               AND (n.delivery_lease_until IS NULL OR n.delivery_lease_until <= now()) \
             ORDER BY n.id FOR UPDATE OF n SKIP LOCKED",
        )
        .bind(recipient_id.get())
        .fetch_all(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let mut notes = Vec::with_capacity(rows.len());
        for row in rows {
            let id: i64 = row.try_get("id").map_err(repository_db_error)?;
            let lease_token = *Uuid::new_v4().as_bytes();
            sqlx::query(
                "UPDATE offline_notes SET delivery_lease_until = now() + interval '30 seconds', \
                 delivery_lease_token = $2 WHERE id = $1",
            )
            .bind(id)
            .bind(lease_token.as_slice())
            .execute(&mut *transaction)
            .await
            .map_err(repository_db_error)?;
            let nickname: Option<String> = row
                .try_get("nickname_display")
                .map_err(repository_db_error)?;
            let message: Vec<u8> = row.try_get("message").map_err(repository_db_error)?;
            notes.push(OfflineNote {
                id,
                lease_token,
                sender_nickname: nickname.unwrap_or_default().into_bytes(),
                message,
            });
        }
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(notes)
    }

    async fn ack_offline_note_inner(
        &self,
        claim: OfflineNoteClaim,
    ) -> Result<bool, RepositoryError> {
        let result = sqlx::query(
            "UPDATE offline_notes SET delivered_at = now(), delivery_lease_until = NULL, \
             delivery_lease_token = NULL WHERE id = $1 AND delivered_at IS NULL \
             AND delivery_lease_token = $2",
        )
        .bind(claim.id)
        .bind(claim.lease_token.as_slice())
        .execute(&self.pool)
        .await
        .map_err(repository_db_error)?;
        Ok(result.rows_affected() == 1)
    }

    async fn accept_offline_note_inner(
        &self,
        request: OfflineNoteRequest,
    ) -> Result<OfflineNoteCommit, RepositoryError> {
        if request.message.is_empty() || request.message.len() > 128 || request.message.contains(&0)
        {
            return Err(RepositoryError::CorruptData);
        }
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let recipient = sqlx::query("SELECT status FROM accounts WHERE id = $1 FOR SHARE")
            .bind(request.recipient_id.get())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(repository_db_error)?
            .ok_or(RepositoryError::NotFound)?;
        let status: String = recipient.try_get("status").map_err(repository_db_error)?;
        if status != "active" {
            return Err(RepositoryError::AccountInactive);
        }

        let inserted = sqlx::query(
            "INSERT INTO offline_notes \
             (sender_account_id, recipient_account_id, operation_id, message) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (sender_account_id, operation_id) \
             DO NOTHING RETURNING id",
        )
        .bind(request.sender_id.get())
        .bind(request.recipient_id.get())
        .bind(request.operation_id.as_slice())
        .bind(&request.message)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .is_some();

        if inserted {
            // The note row and this debit commit together. A failed predicate rolls back the
            // insert, so an unaccepted note can never consume Pang.
            let row = sqlx::query(
                "UPDATE profiles SET pang = pang - 10, updated_at = now() \
                 WHERE account_id = $1 AND pang >= 10 RETURNING pang",
            )
            .bind(request.sender_id.get())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(repository_db_error)?
            .ok_or(RepositoryError::BalanceInsufficient)?;
            let pang: i64 = row.try_get("pang").map_err(repository_db_error)?;
            let pang = u64::try_from(pang).map_err(|_| RepositoryError::CorruptData)?;
            transaction.commit().await.map_err(repository_db_error)?;
            Ok(OfflineNoteCommit {
                pang,
                accepted: true,
            })
        } else {
            // A retry returns the already durable balance without charging again. The unique
            // operation key is intentionally scoped to the sender account.
            let row = sqlx::query("SELECT pang FROM profiles WHERE account_id = $1")
                .bind(request.sender_id.get())
                .fetch_one(&mut *transaction)
                .await
                .map_err(repository_db_error)?;
            let pang: i64 = row.try_get("pang").map_err(repository_db_error)?;
            let pang = u64::try_from(pang).map_err(|_| RepositoryError::CorruptData)?;
            transaction.commit().await.map_err(repository_db_error)?;
            Ok(OfflineNoteCommit {
                pang,
                accepted: false,
            })
        }
    }

    async fn load_player_snapshot_inner(
        &self,
        account_id: AccountId,
    ) -> Result<PlayerSnapshot, RepositoryError> {
        self.load_player_snapshot_with_checkpoint(account_id, std::future::ready(()))
            .await
    }

    async fn load_player_snapshot_with_checkpoint<F>(
        &self,
        account_id: AccountId,
        after_snapshot_begins: F,
    ) -> Result<PlayerSnapshot, RepositoryError>
    where
        F: std::future::Future<Output = ()>,
    {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        sqlx::query!("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *transaction)
            .await
            .map_err(repository_db_error)?;
        let account = sqlx::query_as!(
            AccountRow,
            "SELECT id, username_display, username_normalized, status FROM accounts WHERE id = $1",
            account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .ok_or(RepositoryError::NotFound)?;
        // The first SELECT establishes PostgreSQL's repeatable-read snapshot. Tests pause
        // here so a committed mutation can be placed deterministically before later projections.
        after_snapshot_begins.await;
        let profile = sqlx::query_as!(
            PlayerProfileRow,
            "SELECT account_id, nickname_display, setup_state, pang, points, experience, \
                    selected_character_id \
             FROM profiles WHERE account_id = $1",
            account_id.get()
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let character_limit =
            i64::try_from(MAX_PLAYER_CHARACTERS + 1).map_err(|_| RepositoryError::CorruptData)?;
        let characters = sqlx::query_as!(
            CharacterRow,
            "SELECT id, account_id, item_type_id, starter_key FROM characters \
             WHERE account_id = $1 ORDER BY id LIMIT $2",
            account_id.get(),
            character_limit
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let inventory_limit =
            i64::try_from(MAX_PLAYER_INVENTORY + 1).map_err(|_| RepositoryError::CorruptData)?;
        let inventory = sqlx::query_as!(
            InventoryRow,
            "SELECT id, account_id, item_type_id, quantity, starter_key, inventory_class, \
                    durability, expires_at \
             FROM inventory_items WHERE account_id = $1 ORDER BY id LIMIT $2",
            account_id.get(),
            inventory_limit
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let equipment = sqlx::query_as!(
            EquipmentRow,
            "SELECT id, account_id, character_id, club_item_id, ball_item_id, version \
             FROM equipment_sets WHERE account_id = $1",
            account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .ok_or(RepositoryError::CorruptData)?;
        let snapshot = player_snapshot_from_rows(
            account_id, account, profile, characters, inventory, equipment,
        )?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(snapshot)
    }

    /// Credits an operator balance grant under a row lock so a concurrent reward cannot be lost.
    async fn grant_balance_inner(
        &self,
        account_id: AccountId,
        grant: BalanceGrant,
    ) -> Result<AccountBalances, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let current = sqlx::query!(
            "SELECT pang, points FROM profiles WHERE account_id = $1 FOR UPDATE",
            account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .ok_or(RepositoryError::NotFound)?;
        let pang = operator_balance_add(current.pang, grant.pang)?;
        let points = operator_balance_add(current.points, grant.points)?;
        let updated = sqlx::query!(
            "UPDATE profiles SET pang = $2, points = $3, updated_at = now() WHERE account_id = $1",
            account_id.get(),
            pang,
            points
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        if updated.rows_affected() != 1 {
            return Err(RepositoryError::NotFound);
        }
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(AccountBalances {
            pang: u64::try_from(pang).map_err(|_| RepositoryError::CorruptData)?,
            points: u64::try_from(points).map_err(|_| RepositoryError::CorruptData)?,
        })
    }

    async fn set_status_inner(
        &self,
        account_id: AccountId,
        status: AccountStatus,
        now: SystemTime,
    ) -> Result<(), RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let status_text = account_status_text(status);
        let now = system_time(now);
        let result = sqlx::query!(
            "UPDATE accounts SET status = $2, updated_at = $3 WHERE id = $1",
            account_id.get(),
            status_text,
            now
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        if result.rows_affected() == 0 {
            return Err(RepositoryError::NotFound);
        }
        if status != AccountStatus::Active {
            sqlx::query!(
                "UPDATE handover_sessions SET revoked_at = $2 \
                 WHERE account_id = $1 AND consumed_at IS NULL AND revoked_at IS NULL",
                account_id.get(),
                now
            )
            .execute(&mut *transaction)
            .await
            .map_err(repository_db_error)?;
        }
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(())
    }

    async fn issue_message_eligibility_inner(
        &self,
        eligibility: NewMessageEligibility,
    ) -> Result<(), HandoverError> {
        if eligibility.nickname.is_empty()
            || eligibility.nickname.len() > 22
            || eligibility.expires_at <= eligibility.issued_at
        {
            return Err(HandoverError::Invalid);
        }
        let mut transaction = self.pool.begin().await.map_err(handover_db_error)?;
        let status = sqlx::query_scalar!(
            "SELECT status FROM accounts WHERE id = $1 FOR UPDATE",
            eligibility.account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(handover_db_error)?
        .ok_or(HandoverError::Invalid)?;
        if parse_account_status(&status)
            .map_err(|_| HandoverError::Storage(StorageFault::Decode))?
            != AccountStatus::Active
        {
            return Err(HandoverError::AccountInactive);
        }
        sqlx::query(
            "INSERT INTO message_login_eligibility (account_id, nickname, peer_ip, issued_at, expires_at) \
             VALUES ($1, $2, $3::inet, $4, $5) \
             ON CONFLICT (account_id, nickname, peer_ip) DO UPDATE SET issued_at = EXCLUDED.issued_at, expires_at = EXCLUDED.expires_at",
        )
        .bind(eligibility.account_id.get())
        .bind(eligibility.nickname)
        .bind(eligibility.peer_ip.to_string())
        .bind(system_time(eligibility.issued_at))
        .bind(system_time(eligibility.expires_at))
        .execute(&mut *transaction)
        .await
        .map_err(handover_db_error)?;
        transaction.commit().await.map_err(handover_db_error)
    }

    async fn issue_handover_inner(&self, handover: NewHandover) -> Result<(), HandoverError> {
        if handover.expires_at <= handover.issued_at {
            return Err(HandoverError::Invalid);
        }
        let mut transaction = self.pool.begin().await.map_err(handover_db_error)?;
        let status = sqlx::query_scalar!(
            "SELECT status FROM accounts WHERE id = $1 FOR UPDATE",
            handover.account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(handover_db_error)?
        .ok_or(HandoverError::Invalid)?;
        if parse_account_status(&status)
            .map_err(|_| HandoverError::Storage(StorageFault::Decode))?
            != AccountStatus::Active
        {
            return Err(HandoverError::AccountInactive);
        }
        let issued_at = system_time(handover.issued_at);
        let expires_at = system_time(handover.expires_at);
        sqlx::query!(
            "INSERT INTO handover_sessions \
             (id, account_id, token_digest, target, source_address_prefix, issued_at, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
            handover.id.get(),
            handover.account_id.get(),
            handover.digest.as_bytes().as_slice(),
            service_kind_text(handover.target),
            handover.source_address_prefix.as_str(),
            issued_at,
            expires_at
        )
        .execute(&mut *transaction)
        .await
        .map_err(handover_db_error)?;
        transaction.commit().await.map_err(handover_db_error)?;
        Ok(())
    }

    async fn consume_handover_inner(
        &self,
        request: ConsumeHandover,
    ) -> Result<AuthenticatedSession, HandoverError> {
        let mut transaction = self.pool.begin().await.map_err(handover_db_error)?;
        let row = sqlx::query_as!(
            HandoverRow,
            "SELECT h.account_id, h.token_digest, h.target, h.issued_at, h.expires_at, \
                    h.consumed_at, h.revoked_at, a.status \
             FROM handover_sessions h JOIN accounts a ON a.id = h.account_id \
             WHERE h.id = $1 FOR UPDATE OF h",
            request.id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(handover_db_error)?
        .ok_or(HandoverError::Invalid)?;

        let expected = HandoverDigest::from_slice(&row.token_digest)
            .map_err(|_| HandoverError::Storage(StorageFault::Decode))?;
        if !bool::from(expected.as_bytes().ct_eq(request.digest.as_bytes())) {
            return Err(HandoverError::Invalid);
        }
        let target = parse_service_kind(&row.target)
            .map_err(|_| HandoverError::Storage(StorageFault::Decode))?;
        if target != request.target {
            return Err(HandoverError::WrongTarget);
        }
        let now = system_time(request.now);
        if now < row.issued_at || now >= row.expires_at {
            return Err(HandoverError::Expired);
        }
        if parse_account_status(&row.status)
            .map_err(|_| HandoverError::Storage(StorageFault::Decode))?
            != AccountStatus::Active
        {
            return Err(HandoverError::AccountInactive);
        }
        if row.consumed_at.is_some() || row.revoked_at.is_some() {
            return Err(HandoverError::AlreadyConsumed);
        }
        sqlx::query!(
            "UPDATE handover_sessions SET consumed_at = $2 WHERE id = $1",
            request.id.get(),
            now
        )
        .execute(&mut *transaction)
        .await
        .map_err(handover_db_error)?;
        transaction.commit().await.map_err(handover_db_error)?;
        Ok(AuthenticatedSession {
            account_id: AccountId::new(row.account_id)
                .map_err(|_| HandoverError::Storage(StorageFault::Decode))?,
            handover_id: request.id,
        })
    }

    async fn begin_solo_inner(
        &self,
        request: BeginSoloMatch,
    ) -> Result<BeginSoloMatchOutcome, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let account_status = sqlx::query_scalar!(
            "SELECT status FROM accounts WHERE id = $1 FOR NO KEY UPDATE",
            request.account_id().get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        match account_status.as_deref() {
            Some("active") => {}
            Some(_) => return Err(MatchRepositoryError::InvalidStatus),
            None => return Err(MatchRepositoryError::WrongAccount),
        }
        let catalog_fingerprint = request.catalog_fingerprint();
        let seed = request.seed();
        let inserted = sqlx::query!(
            "INSERT INTO matches \
             (id, result_commit_key, course_id, hole, hole_mode, par, catalog_sha256, seed, weather, \
              wind_speed_tenths, wind_angle_degrees) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) ON CONFLICT DO NOTHING",
            request.match_id().get(),
            request.result_key().get(),
            i64::from(request.config().course_id().get()),
            i16::from(request.config().hole_count()),
            i16::from(request.config().hole_mode()),
            i16::from(request.config().par()),
            catalog_fingerprint.as_bytes().as_slice(),
            seed.as_bytes().as_slice(),
            weather_text(request.weather()),
            i16::try_from(request.wind().speed_tenths())
                .map_err(|_| MatchRepositoryError::CorruptData)?,
            i16::try_from(request.wind().angle_degrees())
                .map_err(|_| MatchRepositoryError::CorruptData)?
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;

        if inserted.rows_affected() == 1 {
            sqlx::query!(
                "INSERT INTO match_players \
                 (match_id, account_id, participant_order, player_result_key) \
                 VALUES ($1, $2, 0, $3)",
                request.match_id().get(),
                request.account_id().get(),
                request.result_key().get()
            )
            .execute(&mut *transaction)
            .await
            .map_err(match_db_error)?;
            sqlx::query!(
                "INSERT INTO match_audit_events (match_id, account_id, event, outcome) \
                 VALUES ($1, $2, 'started', 'success')",
                request.match_id().get(),
                request.account_id().get()
            )
            .execute(&mut *transaction)
            .await
            .map_err(match_db_error)?;
            transaction.commit().await.map_err(match_db_error)?;
            return Ok(BeginSoloMatchOutcome::Begun);
        }

        let rows = sqlx::query_as!(
            MatchPersistenceRow,
            r#"SELECT m.id AS "id!", m.result_commit_key AS "result_commit_key!",
                      m.course_id AS "course_id!", m.hole AS "hole!", m.hole_mode AS "hole_mode!", m.par AS "par!",
                      m.catalog_sha256 AS "catalog_sha256!", m.seed AS "seed!",
                      m.weather AS "weather!",
                      m.wind_speed_tenths AS "wind_speed_tenths!",
                      m.wind_angle_degrees AS "wind_angle_degrees!",
                      m.mode AS "mode!", m.reward_formula AS "reward_formula!",
                      m.status AS "status!", mp.account_id AS "account_id!",
                      mp.participant_order AS "participant_order!",
                      mp.player_result_key AS "player_result_key!",
                      mp.strokes AS "strokes?", mp.score AS "score?",
                      mp.pang_reward AS "pang_reward?",
                      mp.experience_reward AS "experience_reward?",
                      mp.pang_balance_after AS "pang_balance_after?",
                      mp.experience_balance_after AS "experience_balance_after?"
               FROM matches m JOIN match_players mp ON mp.match_id = m.id
               WHERE m.id = $1 OR m.result_commit_key = $2 FOR UPDATE OF m, mp"#,
            request.match_id().get(),
            request.result_key().get()
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if rows.iter().any(|row| row.mode != "solo_practice") {
            return Err(MatchRepositoryError::WrongMode);
        }
        let [row] = rows.as_slice() else {
            return Err(MatchRepositoryError::InputDrift);
        };
        if !row.matches_begin(&request)? {
            return Err(MatchRepositoryError::InputDrift);
        }
        transaction.commit().await.map_err(match_db_error)?;
        Ok(BeginSoloMatchOutcome::Existing)
    }

    async fn mark_solo_in_game_inner(
        &self,
        request: MarkSoloInGame,
    ) -> Result<MarkSoloInGameOutcome, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let row = lock_match(&mut transaction, request.match_id()).await?;
        validate_authority(&row, request.account_id(), request.result_key())?;
        let outcome = match row.status.as_str() {
            "loading" => {
                let updated = sqlx::query!(
                    "UPDATE matches SET status = 'in_game' WHERE id = $1 AND status = 'loading'",
                    request.match_id().get()
                )
                .execute(&mut *transaction)
                .await
                .map_err(match_db_error)?;
                if updated.rows_affected() != 1 {
                    return Err(MatchRepositoryError::Storage(
                        StorageFault::UnexpectedRowCount,
                    ));
                }
                MarkSoloInGameOutcome::Marked
            }
            "in_game" => MarkSoloInGameOutcome::Existing,
            "committed" | "aborted" | "results_pending" => {
                return Err(MatchRepositoryError::InvalidStatus);
            }
            _ => return Err(MatchRepositoryError::CorruptData),
        };
        transaction.commit().await.map_err(match_db_error)?;
        Ok(outcome)
    }

    async fn abort_match_inner(
        &self,
        request: AbortMatch,
    ) -> Result<AbortMatchOutcome, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let row = lock_match(&mut transaction, request.match_id()).await?;
        validate_authority(&row, request.account_id(), request.result_key())?;
        match row.status.as_str() {
            "committed" => {
                let result = row.persisted_result()?;
                transaction.commit().await.map_err(match_db_error)?;
                return Ok(AbortMatchOutcome::AlreadyCommitted(result));
            }
            "aborted" => {
                transaction.commit().await.map_err(match_db_error)?;
                return Ok(AbortMatchOutcome::AlreadyAborted);
            }
            "loading" | "in_game" | "results_pending" => {}
            _ => return Err(MatchRepositoryError::CorruptData),
        }
        sqlx::query!(
            "UPDATE match_players SET quit = TRUE WHERE match_id = $1 AND account_id = $2",
            request.match_id().get(),
            request.account_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        sqlx::query!(
            "UPDATE matches SET status = 'aborted', abort_reason = $2, aborted_at = now() \
             WHERE id = $1",
            request.match_id().get(),
            abort_reason_text(request.reason())
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        sqlx::query!(
            "INSERT INTO match_audit_events \
             (match_id, account_id, event, outcome, reason) \
             VALUES ($1, $2, 'aborted', 'success', $3)",
            request.match_id().get(),
            request.account_id().get(),
            abort_reason_text(request.reason())
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        transaction.commit().await.map_err(match_db_error)?;
        Ok(AbortMatchOutcome::Aborted)
    }

    async fn commit_solo_hole_inner(
        &self,
        request: CommitSoloHole,
    ) -> Result<SoloMatchResult, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let row = lock_match(&mut transaction, request.match_id()).await?;
        validate_authority(&row, request.account_id(), request.result_key())?;
        if row.course_id != i64::from(request.config().course_id().get())
            || row.hole != i16::from(request.config().hole_count())
            || row.hole_mode != i16::from(request.config().hole_mode())
            || row.par != i16::from(request.config().par())
        {
            return Err(MatchRepositoryError::WrongConfig);
        }
        match row.status.as_str() {
            "committed" => {
                let result = row.persisted_result()?;
                if result.strokes() != request.strokes() {
                    return Err(MatchRepositoryError::InputDrift);
                }
                transaction.commit().await.map_err(match_db_error)?;
                return Ok(result);
            }
            "aborted" => return Err(MatchRepositoryError::Aborted),
            "in_game" | "results_pending" => {}
            "loading" => return Err(MatchRepositoryError::InvalidStatus),
            _ => return Err(MatchRepositoryError::CorruptData),
        }

        let pending = sqlx::query!(
            "UPDATE matches SET status = 'results_pending' WHERE id = $1",
            request.match_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if pending.rows_affected() != 1 {
            return Err(MatchRepositoryError::Storage(
                StorageFault::UnexpectedRowCount,
            ));
        }

        let balances = sqlx::query!(
            r#"SELECT pang AS "pang!", experience AS "experience!"
               FROM profiles WHERE account_id = $1 FOR UPDATE"#,
            request.account_id().get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(match_db_error)?
        .ok_or(MatchRepositoryError::WrongAccount)?;
        let (old_pang, old_experience) = (balances.pang, balances.experience);
        let reward = synthetic_solo_reward_v1(request.config(), request.strokes())
            .map_err(|_| MatchRepositoryError::CorruptData)?;
        let new_pang = checked_balance_add(old_pang, reward.pang())?;
        let new_experience = checked_balance_add(old_experience, reward.experience())?;
        let pang_delta =
            i64::try_from(reward.pang()).map_err(|_| MatchRepositoryError::BalanceOverflow)?;
        let experience_delta = i64::try_from(reward.experience())
            .map_err(|_| MatchRepositoryError::BalanceOverflow)?;

        let updated = sqlx::query!(
            "UPDATE profiles SET pang = $2, experience = $3, updated_at = now() \
             WHERE account_id = $1 AND pang = $4 AND experience = $5",
            request.account_id().get(),
            new_pang,
            new_experience,
            old_pang,
            old_experience
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if updated.rows_affected() != 1 {
            return Err(MatchRepositoryError::Storage(
                StorageFault::UnexpectedRowCount,
            ));
        }
        sqlx::query!(
            "INSERT INTO currency_ledger \
             (account_id, match_id, idempotency_key, currency, delta, reason, balance_after) \
             VALUES ($1, $2, $3, 'pang', $4, 'solo-v1', $5)",
            request.account_id().get(),
            request.match_id().get(),
            request.result_key().get(),
            pang_delta,
            new_pang
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        sqlx::query!(
            "INSERT INTO progression_ledger \
             (account_id, match_id, idempotency_key, progression, delta, reason, balance_after) \
             VALUES ($1, $2, $3, 'experience', $4, 'solo-v1', $5)",
            request.account_id().get(),
            request.match_id().get(),
            request.result_key().get(),
            experience_delta,
            new_experience
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        sqlx::query!(
            "UPDATE match_players SET strokes = $3, score = $4, place = 1, \
                    completion = 'holed', pang_reward = $5, experience_reward = $6, \
                    pang_balance_after = $7, experience_balance_after = $8 \
             WHERE match_id = $1 AND account_id = $2",
            request.match_id().get(),
            request.account_id().get(),
            i16::try_from(request.strokes().get())
                .map_err(|_| MatchRepositoryError::CorruptData)?,
            reward.score(),
            pang_delta,
            experience_delta,
            new_pang,
            new_experience
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        sqlx::query!(
            "INSERT INTO match_audit_events (match_id, account_id, event, outcome) \
             VALUES ($1, $2, 'committed', 'success')",
            request.match_id().get(),
            request.account_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        sqlx::query!(
            "UPDATE matches SET status = 'committed', committed_at = now() WHERE id = $1",
            request.match_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        let persisted_balances = sqlx::query!(
            r#"SELECT pang AS "pang!", experience AS "experience!"
               FROM profiles WHERE account_id = $1"#,
            request.account_id().get()
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if (persisted_balances.pang, persisted_balances.experience) != (new_pang, new_experience) {
            return Err(MatchRepositoryError::Storage(
                StorageFault::WriteVerification,
            ));
        }
        let result = SoloMatchResult::new(
            request.match_id(),
            request.result_key(),
            request.account_id(),
            request.strokes(),
            reward,
            pangya_domain::ServerBalances::from_persisted(
                checked_match_u64(new_pang)?,
                checked_match_u64(new_experience)?,
            ),
        );
        transaction.commit().await.map_err(match_db_error)?;
        Ok(result)
    }

    async fn begin_stroke_inner(
        &self,
        request: BeginStrokeMatch,
    ) -> Result<BeginStrokeMatchOutcome, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let mut account_ids = request
            .participants()
            .map(|participant| participant.account_id().get());
        account_ids.sort_unstable();
        let accounts = sqlx::query!(
            "SELECT id, status FROM accounts WHERE id = ANY($1) ORDER BY id FOR NO KEY UPDATE",
            &account_ids[..]
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if accounts.len() != 2
            || accounts
                .iter()
                .zip(account_ids)
                .any(|(row, expected)| row.id != expected)
        {
            return Err(MatchRepositoryError::WrongAccount);
        }
        if accounts.iter().any(|row| row.status != "active") {
            return Err(MatchRepositoryError::InvalidStatus);
        }

        let fingerprint = request.catalog_fingerprint();
        let seed = request.seed();
        let inserted = sqlx::query!(
            "INSERT INTO matches \
             (id, result_commit_key, mode, course_id, hole, hole_mode, par, catalog_sha256, seed, weather, \
              wind_speed_tenths, wind_angle_degrees, reward_formula) \
             VALUES ($1, $2, 'stroke_two', $3, $4, $5, $6, $7, $8, $9, $10, $11, 'stroke-two-v1') \
             ON CONFLICT DO NOTHING",
            request.match_id().get(),
            request.result_key().get(),
            i64::from(request.config().course_id().get()),
            i16::from(request.config().hole_count()),
            i16::from(request.config().hole_mode()),
            i16::from(request.config().par()),
            fingerprint.as_bytes().as_slice(),
            seed.as_bytes().as_slice(),
            weather_text(request.weather()),
            i16::try_from(request.wind().speed_tenths())
                .map_err(|_| MatchRepositoryError::CorruptData)?,
            i16::try_from(request.wind().angle_degrees())
                .map_err(|_| MatchRepositoryError::CorruptData)?
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;

        if inserted.rows_affected() == 1 {
            for participant in request.participants() {
                sqlx::query!(
                    "INSERT INTO match_players \
                     (match_id, account_id, participant_order, player_result_key) \
                     VALUES ($1, $2, $3, $4)",
                    request.match_id().get(),
                    participant.account_id().get(),
                    i16::from(participant.roster_order().get()),
                    participant.player_result_key().get()
                )
                .execute(&mut *transaction)
                .await
                .map_err(stroke_begin_db_error)?;
            }
            sqlx::query!(
                "INSERT INTO match_audit_events (match_id, account_id, event, outcome) \
                 VALUES ($1, $2, 'started', 'success')",
                request.match_id().get(),
                request.participants()[0].account_id().get()
            )
            .execute(&mut *transaction)
            .await
            .map_err(match_db_error)?;
            transaction.commit().await.map_err(match_db_error)?;
            return Ok(BeginStrokeMatchOutcome::Begun);
        }

        let player_keys = request
            .participants()
            .map(|participant| participant.player_result_key().get());
        let candidate_ids = sqlx::query_scalar!(
            "SELECT DISTINCT m.id FROM matches m \
             LEFT JOIN match_players mp ON mp.match_id = m.id \
             WHERE m.id = $1 OR m.result_commit_key = $2 OR mp.player_result_key = ANY($3)",
            request.match_id().get(),
            request.result_key().get(),
            &player_keys[..]
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        let [candidate_id] = candidate_ids.as_slice() else {
            return Err(MatchRepositoryError::InputDrift);
        };
        let (row, players) =
            lock_stroke_match(&mut transaction, MatchId::new(*candidate_id)).await?;
        if !matches!(row.matches_stroke_begin(&request, &players), Ok(true)) {
            return Err(MatchRepositoryError::InputDrift);
        }
        transaction.commit().await.map_err(match_db_error)?;
        Ok(BeginStrokeMatchOutcome::Existing)
    }

    async fn mark_stroke_in_game_inner(
        &self,
        request: MarkStrokeInGame,
    ) -> Result<MarkStrokeInGameOutcome, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let (row, players) = lock_stroke_match(&mut transaction, request.match_id()).await?;
        validate_stroke_aggregate(&row, &players, request.result_key())?;
        let outcome = match row.status.as_str() {
            "loading" => {
                let updated = sqlx::query!(
                    "UPDATE matches SET status = 'in_game' WHERE id = $1 AND status = 'loading'",
                    request.match_id().get()
                )
                .execute(&mut *transaction)
                .await
                .map_err(match_db_error)?;
                if updated.rows_affected() != 1 {
                    return Err(MatchRepositoryError::Storage(
                        StorageFault::UnexpectedRowCount,
                    ));
                }
                MarkStrokeInGameOutcome::Marked
            }
            "in_game" => MarkStrokeInGameOutcome::Existing,
            "results_pending" | "committed" | "aborted" => {
                return Err(MatchRepositoryError::InvalidStatus);
            }
            _ => return Err(MatchRepositoryError::CorruptData),
        };
        transaction.commit().await.map_err(match_db_error)?;
        Ok(outcome)
    }

    async fn abort_stroke_inner(
        &self,
        request: AbortStrokeMatch,
    ) -> Result<AbortStrokeMatchOutcome, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let (row, players) = lock_stroke_match(&mut transaction, request.match_id()).await?;
        validate_stroke_aggregate(&row, &players, request.result_key())?;
        match row.status.as_str() {
            "committed" => {
                let result = persisted_stroke_result(&row, &players)?;
                transaction.commit().await.map_err(match_db_error)?;
                return Ok(AbortStrokeMatchOutcome::AlreadyCommitted(result));
            }
            "aborted" => {
                transaction.commit().await.map_err(match_db_error)?;
                return Ok(AbortStrokeMatchOutcome::AlreadyAborted);
            }
            "loading" | "in_game" | "results_pending" => {}
            _ => return Err(MatchRepositoryError::CorruptData),
        }
        let updated_players = sqlx::query!(
            "UPDATE match_players SET quit = TRUE WHERE match_id = $1",
            request.match_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if updated_players.rows_affected() != 2 {
            return Err(MatchRepositoryError::CorruptData);
        }
        let updated_match = sqlx::query!(
            "UPDATE matches SET status = 'aborted', abort_reason = $2, aborted_at = now() \
             WHERE id = $1",
            request.match_id().get(),
            abort_reason_text(request.reason())
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if updated_match.rows_affected() != 1 {
            return Err(MatchRepositoryError::Storage(
                StorageFault::UnexpectedRowCount,
            ));
        }
        sqlx::query!(
            "INSERT INTO match_audit_events \
             (match_id, account_id, event, outcome, reason) \
             VALUES ($1, $2, 'aborted', 'success', $3)",
            request.match_id().get(),
            players[0].account_id,
            abort_reason_text(request.reason())
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        transaction.commit().await.map_err(match_db_error)?;
        Ok(AbortStrokeMatchOutcome::Aborted)
    }

    async fn commit_stroke_match_inner(
        &self,
        request: CommitStrokeMatch,
    ) -> Result<StrokeMatchResult, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let (row, persisted_players) =
            lock_stroke_match(&mut transaction, request.match_id()).await?;
        validate_stroke_commit(&row, &persisted_players, &request)?;
        match row.status.as_str() {
            "committed" => {
                let result = persisted_stroke_result(&row, &persisted_players)?;
                validate_stroke_replay(&result, &request)?;
                transaction.commit().await.map_err(match_db_error)?;
                return Ok(result);
            }
            "aborted" => return Err(MatchRepositoryError::Aborted),
            "in_game" => {}
            "loading" | "results_pending" => return Err(MatchRepositoryError::InvalidStatus),
            _ => return Err(MatchRepositoryError::CorruptData),
        }

        let pending = sqlx::query!(
            "UPDATE matches SET status = 'results_pending' WHERE id = $1 AND status = 'in_game'",
            request.match_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if pending.rows_affected() != 1 {
            return Err(MatchRepositoryError::Storage(
                StorageFault::UnexpectedRowCount,
            ));
        }

        let mut account_ids = request
            .players()
            .map(|player| player.participant().account_id().get());
        account_ids.sort_unstable();
        let profiles = sqlx::query!(
            r#"SELECT account_id, pang AS "pang!", experience AS "experience!"
               FROM profiles WHERE account_id = ANY($1) ORDER BY account_id FOR UPDATE"#,
            &account_ids[..]
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if profiles.len() != 2
            || profiles
                .iter()
                .zip(account_ids)
                .any(|(profile, expected)| profile.account_id != expected)
        {
            return Err(MatchRepositoryError::WrongAccount);
        }
        let balances_by_account: BTreeMap<i64, (i64, i64)> = profiles
            .into_iter()
            .map(|profile| (profile.account_id, (profile.pang, profile.experience)))
            .collect();

        let mut results = Vec::with_capacity(2);
        for player in request.players() {
            let account_id = player.participant().account_id().get();
            let (old_pang, old_experience) = balances_by_account
                .get(&account_id)
                .copied()
                .ok_or(MatchRepositoryError::CorruptData)?;
            let reward =
                synthetic_stroke_reward_v1(request.config(), player.strokes(), player.completion())
                    .map_err(|_| MatchRepositoryError::CorruptData)?;
            let new_pang = checked_balance_add(old_pang, reward.pang())?;
            let new_experience = checked_balance_add(old_experience, reward.experience())?;
            let pang_delta =
                i64::try_from(reward.pang()).map_err(|_| MatchRepositoryError::BalanceOverflow)?;
            let experience_delta = i64::try_from(reward.experience())
                .map_err(|_| MatchRepositoryError::BalanceOverflow)?;

            if reward.pang() != 0 || reward.experience() != 0 {
                let updated = sqlx::query!(
                    "UPDATE profiles SET pang = $2, experience = $3, updated_at = now() \
                     WHERE account_id = $1 AND pang = $4 AND experience = $5",
                    account_id,
                    new_pang,
                    new_experience,
                    old_pang,
                    old_experience
                )
                .execute(&mut *transaction)
                .await
                .map_err(match_db_error)?;
                if updated.rows_affected() != 1 {
                    return Err(MatchRepositoryError::Storage(
                        StorageFault::UnexpectedRowCount,
                    ));
                }
                sqlx::query!(
                    "INSERT INTO currency_ledger \
                     (account_id, match_id, idempotency_key, currency, delta, reason, balance_after) \
                     VALUES ($1, $2, $3, 'pang', $4, 'stroke-two-v1', $5)",
                    account_id,
                    request.match_id().get(),
                    player.participant().player_result_key().get(),
                    pang_delta,
                    new_pang
                )
                .execute(&mut *transaction)
                .await
                .map_err(match_db_error)?;
                sqlx::query!(
                    "INSERT INTO progression_ledger \
                     (account_id, match_id, idempotency_key, progression, delta, reason, balance_after) \
                     VALUES ($1, $2, $3, 'experience', $4, 'stroke-two-v1', $5)",
                    account_id,
                    request.match_id().get(),
                    player.participant().player_result_key().get(),
                    experience_delta,
                    new_experience
                )
                .execute(&mut *transaction)
                .await
                .map_err(match_db_error)?;
            }

            let settled = sqlx::query!(
                "UPDATE match_players SET strokes = $3, score = $4, quit = $5, place = $6, \
                 completion = $7, pang_reward = $8, experience_reward = $9, \
                 pang_balance_after = $10, experience_balance_after = $11 \
                 WHERE match_id = $1 AND account_id = $2 AND player_result_key = $12",
                request.match_id().get(),
                account_id,
                i16::try_from(player.strokes()).map_err(|_| MatchRepositoryError::CorruptData)?,
                reward.score(),
                player.completion().is_forfeit(),
                i16::from(player.place().get()),
                stroke_completion_text(player.completion()),
                pang_delta,
                experience_delta,
                new_pang,
                new_experience,
                player.participant().player_result_key().get()
            )
            .execute(&mut *transaction)
            .await
            .map_err(match_db_error)?;
            if settled.rows_affected() != 1 {
                return Err(MatchRepositoryError::CorruptData);
            }

            if player.completion().is_record_eligible() {
                let score = reward.score().ok_or(MatchRepositoryError::CorruptData)?;
                sqlx::query!(
                    "INSERT INTO course_records \
                     (account_id, course_id, mode, best_score, best_strokes, rounds_completed, \
                      best_match_id, best_player_result_key, first_achieved_at, updated_at) \
                     VALUES ($1, $2, 'stroke_two', $3, $4, 1, $5, $6, now(), now()) \
                     ON CONFLICT (account_id, course_id, mode) DO UPDATE SET \
                       rounds_completed = course_records.rounds_completed + 1, \
                       best_score = CASE WHEN EXCLUDED.best_score < course_records.best_score \
                         OR (EXCLUDED.best_score = course_records.best_score \
                           AND EXCLUDED.best_strokes < course_records.best_strokes) \
                         THEN EXCLUDED.best_score ELSE course_records.best_score END, \
                       best_strokes = CASE WHEN EXCLUDED.best_score < course_records.best_score \
                         OR (EXCLUDED.best_score = course_records.best_score \
                           AND EXCLUDED.best_strokes < course_records.best_strokes) \
                         THEN EXCLUDED.best_strokes ELSE course_records.best_strokes END, \
                       best_match_id = CASE WHEN EXCLUDED.best_score < course_records.best_score \
                         OR (EXCLUDED.best_score = course_records.best_score \
                           AND EXCLUDED.best_strokes < course_records.best_strokes) \
                         THEN EXCLUDED.best_match_id ELSE course_records.best_match_id END, \
                       best_player_result_key = CASE WHEN EXCLUDED.best_score < course_records.best_score \
                         OR (EXCLUDED.best_score = course_records.best_score \
                           AND EXCLUDED.best_strokes < course_records.best_strokes) \
                         THEN EXCLUDED.best_player_result_key \
                         ELSE course_records.best_player_result_key END, \
                       first_achieved_at = CASE WHEN EXCLUDED.best_score < course_records.best_score \
                         OR (EXCLUDED.best_score = course_records.best_score \
                           AND EXCLUDED.best_strokes < course_records.best_strokes) \
                         THEN EXCLUDED.first_achieved_at ELSE course_records.first_achieved_at END, \
                       updated_at = now()",
                    account_id,
                    i64::from(request.config().course_id().get()),
                    score,
                    i16::try_from(player.strokes())
                        .map_err(|_| MatchRepositoryError::CorruptData)?,
                    request.match_id().get(),
                    player.participant().player_result_key().get()
                )
                .execute(&mut *transaction)
                .await
                .map_err(match_db_error)?;
            }

            results.push(StrokePlayerResult::new(
                *player,
                reward,
                ServerBalances::from_persisted(
                    checked_match_u64(new_pang)?,
                    checked_match_u64(new_experience)?,
                ),
            ));
        }

        sqlx::query!(
            "INSERT INTO match_audit_events (match_id, account_id, event, outcome) \
             VALUES ($1, $2, 'committed', 'success')",
            request.match_id().get(),
            request.players()[0].participant().account_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        let committed = sqlx::query!(
            "UPDATE matches SET status = 'committed', committed_at = now() \
             WHERE id = $1 AND status = 'results_pending'",
            request.match_id().get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if committed.rows_affected() != 1 {
            return Err(MatchRepositoryError::Storage(
                StorageFault::UnexpectedRowCount,
            ));
        }
        let [first, second] = results.as_slice() else {
            return Err(MatchRepositoryError::CorruptData);
        };
        let result =
            StrokeMatchResult::new(request.match_id(), request.result_key(), [*first, *second]);
        transaction.commit().await.map_err(match_db_error)?;
        Ok(result)
    }

    async fn abort_incomplete_matches_inner(
        &self,
        limit: IncompleteMatchAbortLimit,
    ) -> Result<u32, MatchRepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(match_db_error)?;
        let fetch_limit = i64::from(limit.get())
            .checked_add(1)
            .ok_or(MatchRepositoryError::RecoveryLimitExceeded)?;
        let rows = sqlx::query!(
            r#"SELECT id AS "match_id!" FROM matches
               WHERE status IN ('loading', 'in_game', 'results_pending')
               ORDER BY created_at, id LIMIT $1 FOR UPDATE"#,
            fetch_limit
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(match_db_error)?;
        if rows.len()
            > usize::try_from(limit.get())
                .map_err(|_| MatchRepositoryError::RecoveryLimitExceeded)?
        {
            return Err(MatchRepositoryError::RecoveryLimitExceeded);
        }
        for row in &rows {
            let players = sqlx::query!(
                "SELECT account_id, participant_order FROM match_players \
                 WHERE match_id = $1 ORDER BY participant_order FOR UPDATE",
                row.match_id
            )
            .fetch_all(&mut *transaction)
            .await
            .map_err(match_db_error)?;
            let starter_account = players
                .first()
                .map(|player| player.account_id)
                .ok_or(MatchRepositoryError::CorruptData)?;
            let updated_players = sqlx::query!(
                "UPDATE match_players SET quit = TRUE WHERE match_id = $1",
                row.match_id
            )
            .execute(&mut *transaction)
            .await
            .map_err(match_db_error)?;
            if updated_players.rows_affected()
                != u64::try_from(players.len()).map_err(|_| MatchRepositoryError::CorruptData)?
            {
                return Err(MatchRepositoryError::Storage(
                    StorageFault::UnexpectedRowCount,
                ));
            }
            sqlx::query!(
                "UPDATE matches SET status = 'aborted', abort_reason = 'startup_recovery', \
                        aborted_at = now() WHERE id = $1",
                row.match_id
            )
            .execute(&mut *transaction)
            .await
            .map_err(match_db_error)?;
            sqlx::query!(
                "INSERT INTO match_audit_events \
                 (match_id, account_id, event, outcome, reason) \
                 VALUES ($1, $2, 'aborted', 'success', 'startup_recovery')",
                row.match_id,
                starter_account
            )
            .execute(&mut *transaction)
            .await
            .map_err(match_db_error)?;
        }
        let count =
            u32::try_from(rows.len()).map_err(|_| MatchRepositoryError::RecoveryLimitExceeded)?;
        transaction.commit().await.map_err(match_db_error)?;
        Ok(count)
    }
}

impl AccountRepository for PgRepository {
    fn load_chat_macros(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<[Vec<u8>; 9], RepositoryError>> {
        Box::pin(self.observed(self.load_chat_macros_inner(account_id)))
    }

    fn save_chat_macros(
        &self,
        account_id: AccountId,
        macros: [Vec<u8>; 9],
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.save_chat_macros_inner(account_id, macros)))
    }

    fn create_account(
        &self,
        request: NewAccount,
    ) -> RepositoryFuture<'_, Result<AccountAggregate, RepositoryError>> {
        Box::pin(self.observed(self.create_account_inner(request, false)))
    }

    fn load_authentication<'a>(
        &'a self,
        username: &'a NormalizedUsername,
    ) -> RepositoryFuture<'a, Result<Option<AuthenticationRecord>, RepositoryError>> {
        Box::pin(self.observed(self.load_authentication_inner(username)))
    }

    fn set_nickname(
        &self,
        account_id: AccountId,
        nickname: Nickname,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.set_nickname_inner(account_id, nickname)))
    }

    fn nickname_available<'a>(
        &'a self,
        nickname: &'a NormalizedNickname,
    ) -> RepositoryFuture<'a, Result<bool, RepositoryError>> {
        Box::pin(self.observed(self.nickname_available_inner(nickname)))
    }

    fn grant_starter(
        &self,
        account_id: AccountId,
        grant: StarterGrant,
    ) -> RepositoryFuture<'_, Result<AccountAggregate, RepositoryError>> {
        Box::pin(self.observed(self.grant_starter_inner(account_id, grant)))
    }

    fn select_starter_character(
        &self,
        account_id: AccountId,
        item_type_id: ItemTypeId,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.select_starter_character_inner(account_id, item_type_id)))
    }

    fn set_status(
        &self,
        account_id: AccountId,
        status: AccountStatus,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.set_status_inner(account_id, status, now)))
    }

    fn grant_balance(
        &self,
        account_id: AccountId,
        grant: BalanceGrant,
    ) -> RepositoryFuture<'_, Result<AccountBalances, RepositoryError>> {
        Box::pin(self.observed(self.grant_balance_inner(account_id, grant)))
    }
}

impl PgRepository {
    async fn load_retail_equipment_inner(
        &self,
        account_id: AccountId,
    ) -> Result<RetailEquipmentState, RepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(repository_db_error)?;
        load_retail_equipment_connection(&mut connection, account_id).await
    }
}

async fn load_retail_equipment_connection(
    connection: &mut PgConnection,
    account_id: AccountId,
) -> Result<RetailEquipmentState, RepositoryError> {
    let rows = sqlx::query_as::<_, RetailEquipmentSlotRow>(
            "SELECT slot_family, slot_index, inventory_item_id, item_type_id, character_id, cut_in_opaque \
             FROM player_equipment_slots WHERE account_id = $1 ORDER BY slot_family, slot_index",
        )
        .bind(account_id.get())
        .fetch_all(&mut *connection)
        .await
        .map_err(repository_db_error)?;
    let character_hair_color = sqlx::query_scalar::<_, i16>(
            "SELECT c.hair_color FROM characters c JOIN profiles p ON p.selected_character_id = c.id WHERE c.account_id = $1",
        )
        .bind(account_id.get())
        .fetch_optional(&mut *connection)
        .await
        .map_err(repository_db_error)?
        .map(|value| u8::try_from(value).map_err(|_| RepositoryError::CorruptData))
        .transpose()?
        .unwrap_or(0);
    let mut state = RetailEquipmentState {
        character_hair_color,
        ..RetailEquipmentState::default()
    };
    let part_rows = sqlx::query!(
            "SELECT s.character_id, s.slot_index, s.item_type_id, s.inventory_item_id \
             FROM character_part_slots s JOIN profiles p ON p.selected_character_id = s.character_id \
             WHERE s.account_id = $1 ORDER BY s.slot_index",
            account_id.get()
        )
        .fetch_all(&mut *connection)
        .await
        .map_err(repository_db_error)?;
    for row in part_rows {
        let character_id =
            CharacterId::new(row.character_id).map_err(|_| RepositoryError::CorruptData)?;
        let index = usize::try_from(row.slot_index).map_err(|_| RepositoryError::CorruptData)?;
        if index >= 24 {
            return Err(RepositoryError::CorruptData);
        }
        let (current_id, mut types, mut ids) = state
            .character_parts
            .filter(|(id, _, _)| *id == character_id)
            .unwrap_or((character_id, [0; 24], [0; 24]));
        types[index] = u32::try_from(row.item_type_id).map_err(|_| RepositoryError::CorruptData)?;
        ids[index] = u32::try_from(row.inventory_item_id.unwrap_or(0))
            .map_err(|_| RepositoryError::CorruptData)?;
        state.character_parts = Some((current_id, types, ids));
    }
    for row in rows {
        let index = usize::try_from(row.slot_index).map_err(|_| RepositoryError::CorruptData)?;
        let item_type =
            u32::try_from(row.item_type_id).map_err(|_| RepositoryError::CorruptData)?;
        match row.slot_family.as_str() {
            "caddie" => {
                if index != 0 || state.caddie.is_some() {
                    return Err(RepositoryError::CorruptData);
                }
                let item_id = row.inventory_item_id.ok_or(RepositoryError::CorruptData)?;
                state.caddie = Some((
                    InventoryItemId::new(item_id).map_err(|_| RepositoryError::CorruptData)?,
                    item_type,
                ));
            }
            "consumable" if index < state.consumables.len() => state.consumables[index] = item_type,
            "decoration" if index < state.decoration.len() => {
                state.decoration[index] = item_type;
                state.decoration_slots[index] = u32::try_from(row.inventory_item_id.unwrap_or(0))
                    .map_err(|_| RepositoryError::CorruptData)?;
            }
            "mascot" => {
                if index != 0 || state.mascot.is_some() {
                    return Err(RepositoryError::CorruptData);
                }
                let item_id = row.inventory_item_id.ok_or(RepositoryError::CorruptData)?;
                state.mascot = Some((
                    InventoryItemId::new(item_id).map_err(|_| RepositoryError::CorruptData)?,
                    item_type,
                ));
            }
            "cut_in" if index == 0 => {
                let character_id = row.character_id.ok_or(RepositoryError::CorruptData)?;
                let character_id =
                    CharacterId::new(character_id).map_err(|_| RepositoryError::CorruptData)?;
                let bytes = row.cut_in_opaque.ok_or(RepositoryError::CorruptData)?;
                let data: [u8; 16] = bytes.try_into().map_err(|_| RepositoryError::CorruptData)?;
                state.cut_in = Some((character_id, data));
            }
            "cut_in" => return Err(RepositoryError::CorruptData),
            _ => return Err(RepositoryError::CorruptData),
        }
    }
    Ok(state)
}

impl PgRepository {
    async fn update_retail_equipment_inner(
        &self,
        account_id: AccountId,
        operation_id: EconomyOperationId,
        expected_version: u32,
        change: RetailEquipmentChange,
    ) -> Result<EconomyCommit<RetailEquipmentState>, RepositoryError> {
        let request_payload = retail_equipment_request_payload(&change);
        let result_character_parts = match change {
            RetailEquipmentChange::CharacterParts {
                character_id,
                type_ids,
                inventory_ids,
                hair_color,
            } => Some((character_id, type_ids, inventory_ids, hair_color)),
            _ => None,
        };
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        lock_retail_equipment_operation(&mut transaction, operation_id).await?;
        if let Some(row) = sqlx::query(
            "SELECT account_id, request_payload, result_projection FROM retail_equipment_operations \
             WHERE operation_id = $1 FOR UPDATE",
        )
        .bind(operation_id.get())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .map(|row| RetailEquipmentOperationRow {
            account_id: row.get("account_id"),
            request_payload: row.get("request_payload"),
            result_projection: row.get("result_projection"),
        })
        {
            // An exact operation key is replayed from the durable ledger even when the current
            // equipment version has advanced since the original commit. The payload and account
            // remain part of the identity check, so key reuse with drift is still refused.
            if row.account_id != account_id.get() || row.request_payload != request_payload {
                return Err(RepositoryError::CorruptData);
            }
            let state = row
                .result_projection
                .as_deref()
                .map(decode_retail_equipment_state)
                .transpose()?
                .ok_or(RepositoryError::CorruptData)?;
            transaction.commit().await.map_err(repository_db_error)?;
            return Ok(EconomyCommit::Replayed(state));
        }
        let version = sqlx::query_scalar!(
            "SELECT version FROM equipment_sets WHERE account_id = $1 FOR UPDATE",
            account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .ok_or(RepositoryError::NotFound)?;
        if version != i64::from(expected_version) {
            return Err(RepositoryError::CorruptData);
        }

        async fn owned(
            transaction: &mut Transaction<'_, Postgres>,
            account_id: AccountId,
            item_id: u32,
            classes: &[&str],
        ) -> Result<Option<(i64, i64)>, RepositoryError> {
            if item_id == 0 {
                return Ok(None);
            }
            let row = sqlx::query!(
                "SELECT id, item_type_id, inventory_class FROM inventory_items \
                 WHERE account_id = $1 AND id = $2 FOR UPDATE",
                account_id.get(),
                i64::from(item_id)
            )
            .fetch_optional(&mut **transaction)
            .await
            .map_err(repository_db_error)?
            .ok_or(RepositoryError::NotFound)?;
            if !classes.iter().any(|class| *class == row.inventory_class) {
                return Err(RepositoryError::CorruptData);
            }
            Ok(Some((row.id, row.item_type_id)))
        }

        match change {
            RetailEquipmentChange::Caddie(item_id) => {
                let value = owned(&mut transaction, account_id, item_id, &["caddie"]).await?;
                sqlx::query!("DELETE FROM player_equipment_slots WHERE account_id = $1 AND slot_family = 'caddie'", account_id.get())
                    .execute(&mut *transaction).await.map_err(repository_db_error)?;
                if let Some((id, item_type_id)) = value {
                    sqlx::query!("INSERT INTO player_equipment_slots (account_id, slot_family, slot_index, inventory_item_id, item_type_id) VALUES ($1, 'caddie', 0, $2, $3)", account_id.get(), id, item_type_id)
                        .execute(&mut *transaction).await.map_err(repository_db_error)?;
                }
            }
            RetailEquipmentChange::Mascot(item_id) => {
                let value = owned(&mut transaction, account_id, item_id, &["mascot"]).await?;
                sqlx::query!("DELETE FROM player_equipment_slots WHERE account_id = $1 AND slot_family = 'mascot'", account_id.get())
                    .execute(&mut *transaction).await.map_err(repository_db_error)?;
                if let Some((id, item_type_id)) = value {
                    sqlx::query!("INSERT INTO player_equipment_slots (account_id, slot_family, slot_index, inventory_item_id, item_type_id) VALUES ($1, 'mascot', 0, $2, $3)", account_id.get(), id, item_type_id)
                        .execute(&mut *transaction).await.map_err(repository_db_error)?;
                }
            }
            RetailEquipmentChange::Consumables(values) => {
                let mut owned_values = Vec::with_capacity(values.len());
                for value in values {
                    owned_values.push(
                        owned_by_type(&mut transaction, account_id, value, "consumable").await?,
                    );
                }
                sqlx::query!("DELETE FROM player_equipment_slots WHERE account_id = $1 AND slot_family = 'consumable'", account_id.get())
                    .execute(&mut *transaction).await.map_err(repository_db_error)?;
                for (index, value) in owned_values.into_iter().enumerate() {
                    if let Some((id, item_type_id)) = value {
                        sqlx::query!("INSERT INTO player_equipment_slots (account_id, slot_family, slot_index, inventory_item_id, item_type_id) VALUES ($1, 'consumable', $2, $3, $4)", account_id.get(), i16::try_from(index).map_err(|_| RepositoryError::CorruptData)?, id, item_type_id)
                            .execute(&mut *transaction).await.map_err(repository_db_error)?;
                    }
                }
            }
            RetailEquipmentChange::Decoration(values) => {
                let mut owned_values = Vec::with_capacity(values.len());
                for value in values {
                    owned_values.push(
                        owned_by_type_any(&mut transaction, account_id, value, &["skin"]).await?,
                    );
                }
                sqlx::query!("DELETE FROM player_equipment_slots WHERE account_id = $1 AND slot_family = 'decoration'", account_id.get())
                    .execute(&mut *transaction).await.map_err(repository_db_error)?;
                for (index, value) in owned_values.into_iter().enumerate() {
                    if let Some((id, item_type_id)) = value {
                        sqlx::query!("INSERT INTO player_equipment_slots (account_id, slot_family, slot_index, inventory_item_id, item_type_id) VALUES ($1, 'decoration', $2, $3, $4)", account_id.get(), i16::try_from(index).map_err(|_| RepositoryError::CorruptData)?, id, item_type_id)
                            .execute(&mut *transaction).await.map_err(repository_db_error)?;
                    }
                }
            }
            RetailEquipmentChange::CutIn { character_id, data } => {
                ensure_owned_character(&mut transaction, account_id, character_id).await?;
                sqlx::query!("DELETE FROM player_equipment_slots WHERE account_id = $1 AND slot_family = 'cut_in'", account_id.get())
                    .execute(&mut *transaction).await.map_err(repository_db_error)?;
                sqlx::query("INSERT INTO player_equipment_slots (account_id, slot_family, slot_index, inventory_item_id, item_type_id, character_id, cut_in_opaque) VALUES ($1, 'cut_in', 0, NULL, 0, $2, $3)")
                    .bind(account_id.get())
                    .bind(character_id.get())
                    .bind(data.as_slice())
                    .execute(&mut *transaction).await.map_err(repository_db_error)?;
            }
            RetailEquipmentChange::CharacterParts {
                character_id,
                type_ids,
                inventory_ids,
                hair_color,
            } => {
                ensure_owned_character(&mut transaction, account_id, character_id).await?;
                for (type_id, inventory_id) in type_ids.into_iter().zip(inventory_ids) {
                    if (type_id == 0) != (inventory_id == 0) {
                        return Err(RepositoryError::CorruptData);
                    }
                    if inventory_id != 0 {
                        let row = owned(
                            &mut transaction,
                            account_id,
                            inventory_id,
                            &["character_part"],
                        )
                        .await?
                        .ok_or(RepositoryError::NotFound)?;
                        if row.1 != i64::from(type_id) {
                            return Err(RepositoryError::CorruptData);
                        }
                    }
                }
                sqlx::query(
                    "UPDATE characters SET hair_color = $1 WHERE account_id = $2 AND id = $3",
                )
                .bind(i16::from(hair_color))
                .bind(account_id.get())
                .bind(character_id.get())
                .execute(&mut *transaction)
                .await
                .map_err(repository_db_error)?;
                sqlx::query!(
                    "DELETE FROM character_part_slots WHERE account_id = $1 AND character_id = $2",
                    account_id.get(),
                    character_id.get()
                )
                .execute(&mut *transaction)
                .await
                .map_err(repository_db_error)?;
                for (index, (type_id, inventory_id)) in
                    type_ids.into_iter().zip(inventory_ids).enumerate()
                {
                    if type_id != 0 {
                        sqlx::query!("INSERT INTO character_part_slots (account_id, character_id, slot_index, inventory_item_id, item_type_id) VALUES ($1, $2, $3, $4, $5)", account_id.get(), character_id.get(), i16::try_from(index).map_err(|_| RepositoryError::CorruptData)?, i64::from(inventory_id), i64::from(type_id))
                            .execute(&mut *transaction).await.map_err(repository_db_error)?;
                    }
                }
            }
        }
        let updated = sqlx::query!("UPDATE equipment_sets SET version = version + 1, updated_at = now() WHERE account_id = $1 AND version = $2 RETURNING version", account_id.get(), i64::from(expected_version))
            .fetch_optional(&mut *transaction).await.map_err(repository_db_error)?
            .ok_or(RepositoryError::CorruptData)?;
        let mut state = load_retail_equipment_connection(&mut transaction, account_id).await?;
        if let Some((character_id, type_ids, inventory_ids, hair_color)) = result_character_parts {
            state.character_parts = Some((character_id, type_ids, inventory_ids));
            state.character_hair_color = hair_color;
        }
        let result_projection = encode_retail_equipment_state(&state);
        sqlx::query(
            "INSERT INTO retail_equipment_operations \
             (operation_id, account_id, request_payload, expected_version, result_version, result_projection) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(operation_id.get())
        .bind(account_id.get())
        .bind(&request_payload)
        .bind(i64::from(expected_version))
        .bind(updated.version)
        .bind(&result_projection)
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(EconomyCommit::Committed(state))
    }
}

async fn lock_retail_equipment_operation(
    transaction: &mut Transaction<'_, Postgres>,
    operation_id: EconomyOperationId,
) -> Result<(), RepositoryError> {
    let key = operation_id.get().to_string();
    sqlx::query!("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", key)
        .execute(&mut **transaction)
        .await
        .map_err(repository_db_error)?;
    Ok(())
}

fn retail_equipment_request_payload(change: &RetailEquipmentChange) -> Vec<u8> {
    let mut payload = Vec::with_capacity(100);
    match change {
        RetailEquipmentChange::Caddie(value) => {
            payload.push(1);
            payload.extend_from_slice(&value.to_le_bytes());
        }
        RetailEquipmentChange::Consumables(values) => {
            payload.push(2);
            for value in values {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
        RetailEquipmentChange::Decoration(values) => {
            payload.push(3);
            for value in values {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
        RetailEquipmentChange::Mascot(value) => {
            payload.push(4);
            payload.extend_from_slice(&value.to_le_bytes());
        }
        RetailEquipmentChange::CutIn { character_id, data } => {
            payload.push(5);
            payload.extend_from_slice(&character_id.get().to_le_bytes());
            payload.extend_from_slice(data);
        }
        RetailEquipmentChange::CharacterParts {
            character_id,
            type_ids,
            inventory_ids,
            hair_color,
        } => {
            payload.push(6);
            payload.extend_from_slice(&character_id.get().to_le_bytes());
            for value in type_ids {
                payload.extend_from_slice(&value.to_le_bytes());
            }
            for value in inventory_ids {
                payload.extend_from_slice(&value.to_le_bytes());
            }
            payload.push(*hair_color);
        }
    }
    payload
}

fn encode_retail_equipment_state(state: &RetailEquipmentState) -> Vec<u8> {
    let mut payload = Vec::with_capacity(256);
    payload.push(1);
    fn option_item(payload: &mut Vec<u8>, value: Option<(InventoryItemId, u32)>) {
        if let Some((id, type_id)) = value {
            payload.push(1);
            payload.extend_from_slice(&id.get().to_le_bytes());
            payload.extend_from_slice(&type_id.to_le_bytes());
        } else {
            payload.push(0);
        }
    }
    option_item(&mut payload, state.caddie);
    for value in state.consumables {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    for value in state.decoration {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    option_item(&mut payload, state.mascot);
    if let Some((character_id, data)) = state.cut_in {
        payload.push(1);
        payload.extend_from_slice(&character_id.get().to_le_bytes());
        payload.extend_from_slice(&data);
    } else {
        payload.push(0);
    }
    payload.push(state.character_hair_color);
    for value in state.decoration_slots {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    if let Some((character_id, type_ids, inventory_ids)) = state.character_parts {
        payload.push(1);
        payload.extend_from_slice(&character_id.get().to_le_bytes());
        for value in type_ids {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        for value in inventory_ids {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    } else {
        payload.push(0);
    }
    payload
}

fn decode_retail_equipment_state(payload: &[u8]) -> Result<RetailEquipmentState, RepositoryError> {
    let mut offset = 0;
    let take = |offset: &mut usize, count: usize| {
        let end = offset
            .checked_add(count)
            .ok_or(RepositoryError::CorruptData)?;
        let bytes = payload
            .get(*offset..end)
            .ok_or(RepositoryError::CorruptData)?;
        *offset = end;
        Ok::<_, RepositoryError>(bytes)
    };
    let version = take(&mut offset, 1)?[0];
    if version != 1 {
        return Err(RepositoryError::CorruptData);
    }
    let item = |offset: &mut usize| -> Result<Option<(InventoryItemId, u32)>, RepositoryError> {
        match take(offset, 1)?[0] {
            0 => Ok(None),
            1 => {
                let id = i64::from_le_bytes(
                    take(offset, 8)?
                        .try_into()
                        .map_err(|_| RepositoryError::CorruptData)?,
                );
                let type_id = u32::from_le_bytes(
                    take(offset, 4)?
                        .try_into()
                        .map_err(|_| RepositoryError::CorruptData)?,
                );
                Ok(Some((
                    InventoryItemId::new(id).map_err(|_| RepositoryError::CorruptData)?,
                    type_id,
                )))
            }
            _ => Err(RepositoryError::CorruptData),
        }
    };
    let caddie = item(&mut offset)?;
    let mut consumables = [0; 10];
    for value in &mut consumables {
        *value = u32::from_le_bytes(
            take(&mut offset, 4)?
                .try_into()
                .map_err(|_| RepositoryError::CorruptData)?,
        );
    }
    let mut decoration = [0; 6];
    for value in &mut decoration {
        *value = u32::from_le_bytes(
            take(&mut offset, 4)?
                .try_into()
                .map_err(|_| RepositoryError::CorruptData)?,
        );
    }
    let mascot = item(&mut offset)?;
    let cut_in = match take(&mut offset, 1)?[0] {
        0 => None,
        1 => {
            let id = i64::from_le_bytes(
                take(&mut offset, 8)?
                    .try_into()
                    .map_err(|_| RepositoryError::CorruptData)?,
            );
            let data = take(&mut offset, 16)?
                .try_into()
                .map_err(|_| RepositoryError::CorruptData)?;
            Some((
                CharacterId::new(id).map_err(|_| RepositoryError::CorruptData)?,
                data,
            ))
        }
        _ => return Err(RepositoryError::CorruptData),
    };
    let character_hair_color = take(&mut offset, 1)?[0];
    let mut decoration_slots = [0; 6];
    for value in &mut decoration_slots {
        *value = u32::from_le_bytes(
            take(&mut offset, 4)?
                .try_into()
                .map_err(|_| RepositoryError::CorruptData)?,
        );
    }
    let character_parts = match take(&mut offset, 1)?[0] {
        0 => None,
        1 => {
            let id = i64::from_le_bytes(
                take(&mut offset, 8)?
                    .try_into()
                    .map_err(|_| RepositoryError::CorruptData)?,
            );
            let mut type_ids = [0; 24];
            for value in &mut type_ids {
                *value = u32::from_le_bytes(
                    take(&mut offset, 4)?
                        .try_into()
                        .map_err(|_| RepositoryError::CorruptData)?,
                );
            }
            let mut inventory_ids = [0; 24];
            for value in &mut inventory_ids {
                *value = u32::from_le_bytes(
                    take(&mut offset, 4)?
                        .try_into()
                        .map_err(|_| RepositoryError::CorruptData)?,
                );
            }
            Some((
                CharacterId::new(id).map_err(|_| RepositoryError::CorruptData)?,
                type_ids,
                inventory_ids,
            ))
        }
        _ => return Err(RepositoryError::CorruptData),
    };
    if offset != payload.len() {
        return Err(RepositoryError::CorruptData);
    }
    Ok(RetailEquipmentState {
        caddie,
        consumables,
        decoration,
        mascot,
        cut_in,
        character_hair_color,
        decoration_slots,
        character_parts,
    })
}

async fn ensure_owned_character(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
    character_id: CharacterId,
) -> Result<(), RepositoryError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM characters WHERE account_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(account_id.get())
    .bind(character_id.get())
    .fetch_optional(&mut **transaction)
    .await
    .map_err(repository_db_error)?
    .ok_or(RepositoryError::NotFound)?;
    Ok(())
}

async fn owned_by_type(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
    item_type_id: u32,
    class: &str,
) -> Result<Option<(i64, i64)>, RepositoryError> {
    owned_by_type_any(transaction, account_id, item_type_id, &[class]).await
}

async fn owned_by_type_any(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
    item_type_id: u32,
    classes: &[&str],
) -> Result<Option<(i64, i64)>, RepositoryError> {
    if item_type_id == 0 {
        return Ok(None);
    }
    let classes: Vec<String> = classes.iter().map(|class| (*class).to_owned()).collect();
    let rows = sqlx::query!("SELECT id, item_type_id, inventory_class FROM inventory_items WHERE account_id = $1 AND item_type_id = $2 AND inventory_class = ANY($3) ORDER BY id FOR UPDATE", account_id.get(), i64::from(item_type_id), &classes)
        .fetch_all(&mut **transaction).await.map_err(repository_db_error)?;
    let row = rows.into_iter().next().ok_or(RepositoryError::NotFound)?;
    Ok(Some((row.id, row.item_type_id)))
}

impl PgRepository {
    async fn load_my_room_inner(
        &self,
        account_id: AccountId,
    ) -> Result<MyRoomProjection, RepositoryError> {
        let mut connection = self.pool.acquire().await.map_err(repository_db_error)?;
        let rows = sqlx::query(
            "SELECT unknown_prefix, item_type_id, unknown_suffix FROM my_room_furniture \
             WHERE account_id = $1 ORDER BY slot_index LIMIT 1024",
        )
        .bind(account_id.get())
        .fetch_all(&mut *connection)
        .await
        .map_err(repository_db_error)?;
        let mut furniture = Vec::with_capacity(rows.len());
        for row in rows {
            let prefix: Vec<u8> = row.try_get("unknown_prefix").map_err(repository_db_error)?;
            let suffix: Vec<u8> = row.try_get("unknown_suffix").map_err(repository_db_error)?;
            if prefix.len() != 4 || suffix.len() != 19 {
                return Err(RepositoryError::CorruptData);
            }
            let mut unknown_prefix = [0; 4];
            unknown_prefix.copy_from_slice(&prefix);
            let mut unknown_suffix = [0; 19];
            unknown_suffix.copy_from_slice(&suffix);
            let item_type_id: i64 = row.try_get("item_type_id").map_err(repository_db_error)?;
            furniture.push(MyRoomFurniture {
                unknown_prefix,
                item_type_id: u32::try_from(item_type_id)
                    .map_err(|_| RepositoryError::CorruptData)?,
                unknown_suffix,
            });
        }
        let mascot_message = sqlx::query(
            "SELECT m.message FROM mascot_messages m \
             JOIN player_equipment_slots s ON s.account_id = m.account_id \
               AND s.inventory_item_id = m.inventory_item_id \
             WHERE m.account_id = $1 AND s.slot_family = 'mascot' AND s.slot_index = 0",
        )
        .bind(account_id.get())
        .fetch_optional(&mut *connection)
        .await
        .map_err(repository_db_error)?
        .map(|row| row.try_get("message").map_err(repository_db_error))
        .transpose()?;
        Ok(MyRoomProjection {
            furniture,
            mascot_message,
        })
    }

    async fn save_mascot_message_inner(
        &self,
        account_id: AccountId,
        update: MascotMessageUpdate,
    ) -> Result<(), RepositoryError> {
        if update.message.is_empty() || update.message.len() > 30 || update.message.contains(&0) {
            return Err(RepositoryError::CorruptData);
        }
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let owned = sqlx::query_scalar::<_, i64>(
            "SELECT id FROM inventory_items WHERE account_id = $1 AND id = $2 AND inventory_class = 'mascot' FOR UPDATE",
        )
        .bind(account_id.get())
        .bind(update.inventory_item_id.get())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        if owned.is_none() {
            return Err(RepositoryError::NotFound);
        }
        sqlx::query(
            "INSERT INTO mascot_messages (account_id, inventory_item_id, message) VALUES ($1, $2, $3) \
             ON CONFLICT (account_id, inventory_item_id) DO UPDATE SET message = EXCLUDED.message, updated_at = now()",
        )
        .bind(account_id.get())
        .bind(update.inventory_item_id.get())
        .bind(&update.message)
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)
    }
}

impl PgRepository {
    async fn load_recent_players_inner(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<RecentPlayer>, RepositoryError> {
        let rows = sqlx::query(
            "SELECT recent_account_id, nickname, seen_at FROM retail_recent_players WHERE account_id = $1 ORDER BY seen_at DESC LIMIT $2",
        )
        .bind(account_id.get())
        .bind(i64::try_from(MAX_RECENT_PLAYERS).map_err(|_| RepositoryError::CorruptData)? )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;
        rows.into_iter()
            .map(|row| {
                let id = AccountId::new(
                    row.try_get::<i64, _>("recent_account_id")
                        .map_err(repository_db_error)?,
                )
                .map_err(|_| RepositoryError::CorruptData)?;
                Ok(RecentPlayer {
                    account_id: id,
                    nickname: row.try_get("nickname").map_err(repository_db_error)?,
                    seen_at: row
                        .try_get::<DateTime<Utc>, _>("seen_at")
                        .map_err(repository_db_error)?
                        .into(),
                })
            })
            .collect()
    }

    async fn record_recent_player_inner(
        &self,
        account_id: AccountId,
        recent: RecentPlayer,
    ) -> Result<(), RepositoryError> {
        if recent.nickname.is_empty()
            || recent.nickname.len() > 21
            || recent.nickname.as_bytes().contains(&0)
        {
            return Err(RepositoryError::CorruptData);
        }
        let mut tx = self.pool.begin().await.map_err(repository_db_error)?;
        sqlx::query(
            "INSERT INTO retail_recent_players (account_id, recent_account_id, nickname, seen_at) VALUES ($1, $2, $3, $4) ON CONFLICT (account_id, recent_account_id) DO UPDATE SET nickname = EXCLUDED.nickname, seen_at = EXCLUDED.seen_at",
        )
        .bind(account_id.get())
        .bind(recent.account_id.get())
        .bind(recent.nickname)
        .bind(system_time(recent.seen_at))
        .execute(&mut *tx)
        .await
        .map_err(repository_db_error)?;
        sqlx::query(
            "DELETE FROM retail_recent_players WHERE account_id = $1 AND recent_account_id NOT IN (SELECT recent_account_id FROM retail_recent_players WHERE account_id = $1 ORDER BY seen_at DESC LIMIT $2)",
        )
        .bind(account_id.get())
        .bind(i64::try_from(MAX_RECENT_PLAYERS).map_err(|_| RepositoryError::CorruptData)? )
        .execute(&mut *tx)
        .await
        .map_err(repository_db_error)?;
        tx.commit().await.map_err(repository_db_error)

async fn login_bonus_claimed_inner(
        &self,
        account_id: AccountId,
        server_day: i64,
    ) -> Result<bool, RepositoryError> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM login_bonus_claims WHERE account_id = $1 AND server_day = $2)",
        )
        .bind(account_id.get())
        .bind(server_day)
        .fetch_one(&self.pool)
        .await
        .map_err(repository_db_error)
    }

    async fn claim_login_bonus_inner(
        &self,
        account_id: AccountId,
        server_day: i64,
        calendar_day: u32,
        reward: LoginBonusReward,
    ) -> Result<LoginBonusClaim, RepositoryError> {
        if server_day < 0 || calendar_day == 0 || reward.quantity == 0 {
            return Err(RepositoryError::CorruptData);
        }
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        // The account lock serializes this reward with all other inventory mutations for the
        // account. The primary key below is still the durable exactly-once fence across sessions.
        sqlx::query("SELECT id FROM accounts WHERE id = $1 FOR UPDATE")
            .bind(account_id.get())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(repository_db_error)?
            .ok_or(RepositoryError::NotFound)?;
        if let Some(row) = sqlx::query(
            "SELECT inventory_item_id, quantity, calendar_day FROM login_bonus_claims \
             WHERE account_id = $1 AND server_day = $2 FOR UPDATE",
        )
        .bind(account_id.get())
        .bind(server_day)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        {
            let inventory_id = row.try_get::<i64, _>("inventory_item_id").map_err(|_| RepositoryError::CorruptData)?;
            let quantity = row.try_get::<i32, _>("quantity").map_err(|_| RepositoryError::CorruptData)?;
            let day = row.try_get::<i32, _>("calendar_day").map_err(|_| RepositoryError::CorruptData)?;
            transaction.commit().await.map_err(repository_db_error)?;
            return Ok(LoginBonusClaim {
                already_claimed: true,
                inventory_item_id: InventoryItemId::new(inventory_id).map_err(|_| RepositoryError::CorruptData)?,
                quantity_after: u32::try_from(quantity).map_err(|_| RepositoryError::CorruptData)?,
                calendar_day: u32::try_from(day).map_err(|_| RepositoryError::CorruptData)?,
            });
        }
        let class = match reward.definition.kind {
            ItemKind::ClubSet => "club_set",
            ItemKind::Ball => "ball",
            ItemKind::Consumable => "consumable",
            ItemKind::CharacterPart => "character_part",
            ItemKind::Caddie => "caddie",
            ItemKind::CaddieItem => "caddie_item",
            ItemKind::Mascot => "mascot",
            ItemKind::Card => "card",
            ItemKind::Furniture => "furniture",
            ItemKind::Skin => "skin",
            ItemKind::HairStyle => "hair_style",
            ItemKind::SetItem => "set_item",
            ItemKind::Character => return Err(RepositoryError::CorruptData),
        };
        let item_type_id = i64::from(reward.definition.type_id.get());
        let quantity = i64::from(reward.quantity);
        let inventory = if class == "consumable" {
            if let Some(row) = sqlx::query(
                "SELECT id, quantity FROM inventory_items WHERE account_id = $1 AND item_type_id = $2 AND inventory_class = 'consumable' FOR UPDATE",
            )
            .bind(account_id.get())
            .bind(item_type_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(repository_db_error)? {
                let id = row.try_get::<i64, _>("id").map_err(|_| RepositoryError::CorruptData)?;
                let before = row.try_get::<i64, _>("quantity").map_err(|_| RepositoryError::CorruptData)?;
                let after = before.checked_add(quantity).ok_or(RepositoryError::CorruptData)?;
                sqlx::query("UPDATE inventory_items SET quantity = $1, updated_at = now() WHERE account_id = $2 AND id = $3")
                    .bind(after).bind(account_id.get()).bind(id).execute(&mut *transaction).await.map_err(repository_db_error)?;
                (id, after)
            } else {
                let id = sqlx::query_scalar::<_, i64>(
                    "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) VALUES ($1, $2, $3, $4, 'consumable') RETURNING id",
                )
                .bind(account_id.get()).bind(item_type_id).bind(format!("login_bonus.{server_day}" )).bind(quantity)
                .fetch_one(&mut *transaction).await.map_err(repository_db_error)?;
                (id, quantity)
            }
        } else {
            let id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO inventory_items (account_id, item_type_id, starter_key, quantity, inventory_class) VALUES ($1, $2, $3, 1, $4) RETURNING id",
            )
            .bind(account_id.get()).bind(item_type_id).bind(format!("login_bonus.{server_day}" )).bind(class)
            .fetch_one(&mut *transaction).await.map_err(repository_db_error)?;
            (id, 1)
        };
        sqlx::query(
            "INSERT INTO login_bonus_claims (account_id, server_day, calendar_day, item_type_id, quantity, inventory_item_id) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(account_id.get())
        .bind(server_day)
        .bind(i32::try_from(calendar_day).map_err(|_| RepositoryError::CorruptData)?)
        .bind(item_type_id)
        .bind(i32::try_from(reward.quantity).map_err(|_| RepositoryError::CorruptData)?)
        .bind(inventory.0)
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(LoginBonusClaim {
            already_claimed: false,
            inventory_item_id: InventoryItemId::new(inventory.0).map_err(|_| RepositoryError::CorruptData)?,
            quantity_after: u32::try_from(inventory.1).map_err(|_| RepositoryError::CorruptData)?,
            calendar_day,
        })    }
}

impl PlayerRepository for PgRepository {
    fn load_recent_players(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<Vec<RecentPlayer>, RepositoryError>> {
        Box::pin(self.observed(self.load_recent_players_inner(account_id)))
    }

    fn record_recent_player(
        &self,
        account_id: AccountId,
        recent: RecentPlayer,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.record_recent_player_inner(account_id, recent)))

fn login_bonus_claimed(
        &self,
        account_id: AccountId,
        server_day: i64,
    ) -> RepositoryFuture<'_, Result<bool, RepositoryError>> {
        Box::pin(self.observed(self.login_bonus_claimed_inner(account_id, server_day)))
    }

    fn claim_login_bonus(
        &self,
        account_id: AccountId,
        server_day: i64,
        calendar_day: u32,
        reward: LoginBonusReward,
    ) -> RepositoryFuture<'_, Result<LoginBonusClaim, RepositoryError>> {
        Box::pin(self.observed(self.claim_login_bonus_inner(account_id, server_day, calendar_day, reward)))    }

    fn claim_offline_notes(
        &self,
        recipient_id: AccountId,
    ) -> RepositoryFuture<'_, Result<Vec<OfflineNote>, RepositoryError>> {
        Box::pin(self.observed(self.claim_offline_notes_inner(recipient_id)))
    }

    fn ack_offline_note(
        &self,
        claim: OfflineNoteClaim,
    ) -> RepositoryFuture<'_, Result<bool, RepositoryError>> {
        Box::pin(self.observed(self.ack_offline_note_inner(claim)))
    }

    fn save_chat_macros(
        &self,
        account_id: AccountId,
        macros: [Vec<u8>; 9],
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.save_chat_macros_inner(account_id, macros)))
    }

    fn load_player_snapshot(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<PlayerSnapshot, RepositoryError>> {
        Box::pin(self.observed(self.load_player_snapshot_inner(account_id)))
    }

    fn load_my_room(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<MyRoomProjection, RepositoryError>> {
        Box::pin(self.observed(self.load_my_room_inner(account_id)))
    }

    fn save_mascot_message(
        &self,
        account_id: AccountId,
        update: MascotMessageUpdate,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.save_mascot_message_inner(account_id, update)))
    }

    fn load_retail_equipment(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<RetailEquipmentState, RepositoryError>> {
        Box::pin(self.observed(self.load_retail_equipment_inner(account_id)))
    }

    fn update_retail_equipment(
        &self,
        account_id: AccountId,
        operation_id: EconomyOperationId,
        expected_version: u32,
        change: RetailEquipmentChange,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<RetailEquipmentState>, RepositoryError>> {
        Box::pin(self.observed(self.update_retail_equipment_inner(
            account_id,
            operation_id,
            expected_version,
            change,
        )))
    }

    fn accept_offline_note(
        &self,
        request: OfflineNoteRequest,
    ) -> RepositoryFuture<'_, Result<OfflineNoteCommit, RepositoryError>> {
        Box::pin(self.observed(self.accept_offline_note_inner(request)))
    }
}

impl MessageEligibilityRepository for PgRepository {
    fn issue_message_eligibility(
        &self,
        eligibility: NewMessageEligibility,
    ) -> RepositoryFuture<'_, Result<(), HandoverError>> {
        Box::pin(self.observed(self.issue_message_eligibility_inner(eligibility)))
    }
}

impl HandoverRepository for PgRepository {
    fn issue(&self, handover: NewHandover) -> RepositoryFuture<'_, Result<(), HandoverError>> {
        Box::pin(self.observed(self.issue_handover_inner(handover)))
    }

    fn consume(
        &self,
        request: ConsumeHandover,
    ) -> RepositoryFuture<'_, Result<AuthenticatedSession, HandoverError>> {
        Box::pin(self.observed(self.consume_handover_inner(request)))
    }
}

impl MatchRepository for PgRepository {
    fn begin_stroke(
        &self,
        request: BeginStrokeMatch,
    ) -> RepositoryFuture<'_, Result<BeginStrokeMatchOutcome, MatchRepositoryError>> {
        Box::pin(self.observed(self.begin_stroke_inner(request)))
    }

    fn mark_stroke_in_game(
        &self,
        request: MarkStrokeInGame,
    ) -> RepositoryFuture<'_, Result<MarkStrokeInGameOutcome, MatchRepositoryError>> {
        Box::pin(self.observed(self.mark_stroke_in_game_inner(request)))
    }

    fn abort_stroke(
        &self,
        request: AbortStrokeMatch,
    ) -> RepositoryFuture<'_, Result<AbortStrokeMatchOutcome, MatchRepositoryError>> {
        Box::pin(self.observed(self.abort_stroke_inner(request)))
    }

    fn commit_stroke_match(
        &self,
        request: CommitStrokeMatch,
    ) -> RepositoryFuture<'_, Result<StrokeMatchResult, MatchRepositoryError>> {
        Box::pin(self.observed(self.commit_stroke_match_inner(request)))
    }

    fn begin_solo(
        &self,
        request: BeginSoloMatch,
    ) -> RepositoryFuture<'_, Result<BeginSoloMatchOutcome, MatchRepositoryError>> {
        Box::pin(self.observed(self.begin_solo_inner(request)))
    }

    fn mark_solo_in_game(
        &self,
        request: MarkSoloInGame,
    ) -> RepositoryFuture<'_, Result<MarkSoloInGameOutcome, MatchRepositoryError>> {
        Box::pin(self.observed(self.mark_solo_in_game_inner(request)))
    }

    fn abort(
        &self,
        request: AbortMatch,
    ) -> RepositoryFuture<'_, Result<AbortMatchOutcome, MatchRepositoryError>> {
        Box::pin(self.observed(self.abort_match_inner(request)))
    }

    fn commit_solo_hole(
        &self,
        request: CommitSoloHole,
    ) -> RepositoryFuture<'_, Result<SoloMatchResult, MatchRepositoryError>> {
        Box::pin(self.observed(self.commit_solo_hole_inner(request)))
    }

    fn abort_incomplete_matches(
        &self,
        limit: IncompleteMatchAbortLimit,
    ) -> RepositoryFuture<'_, Result<u32, MatchRepositoryError>> {
        Box::pin(self.observed(self.abort_incomplete_matches_inner(limit)))
    }
}

#[derive(FromRow)]
struct AuthenticationRow {
    id: i64,
    username_display: String,
    username_normalized: String,
    status: String,
    password_hash: String,
    setup_state: String,
    nickname_display: Option<String>,
}

impl AuthenticationRow {
    fn into_domain(self) -> Result<AuthenticationRecord, RepositoryError> {
        Ok(AuthenticationRecord {
            account: Account {
                id: AccountId::new(self.id).map_err(|_| RepositoryError::CorruptData)?,
                username_display: self.username_display,
                username_normalized: NormalizedUsername::parse(&self.username_normalized)
                    .map_err(|_| RepositoryError::CorruptData)?,
                status: parse_account_status(&self.status)?,
            },
            nickname: self.nickname_display,
            credential_hash: CredentialHash::new(self.password_hash),
            setup_state: parse_setup_state(&self.setup_state)?,
        })
    }
}

#[derive(FromRow)]
struct AccountRow {
    id: i64,
    username_display: String,
    username_normalized: String,
    status: String,
}

struct RetailEquipmentOperationRow {
    account_id: i64,
    request_payload: Vec<u8>,
    result_projection: Option<Vec<u8>>,
}

#[derive(FromRow)]
struct RetailEquipmentSlotRow {
    slot_family: String,
    slot_index: i16,
    inventory_item_id: Option<i64>,
    item_type_id: i64,
    character_id: Option<i64>,
    cut_in_opaque: Option<Vec<u8>>,
}

#[derive(FromRow)]
struct ProfileRow {
    account_id: i64,
    nickname_display: Option<String>,
    setup_state: String,
    pang: i64,
    points: i64,
    experience: i64,
}

#[derive(FromRow)]
struct PlayerProfileRow {
    account_id: i64,
    nickname_display: Option<String>,
    setup_state: String,
    pang: i64,
    points: i64,
    experience: i64,
    selected_character_id: Option<i64>,
}

#[derive(FromRow)]
struct CharacterRow {
    id: i64,
    account_id: i64,
    item_type_id: i64,
    starter_key: String,
}

#[derive(FromRow)]
struct InventoryRow {
    id: i64,
    account_id: i64,
    item_type_id: i64,
    quantity: i64,
    starter_key: String,
    inventory_class: String,
    durability: Option<i64>,
    expires_at: Option<DateTime<Utc>>,
}

#[derive(FromRow)]
struct EquipmentRow {
    id: i64,
    account_id: i64,
    character_id: i64,
    club_item_id: Option<i64>,
    ball_item_id: Option<i64>,
    version: i64,
}

#[derive(FromRow)]
struct HandoverRow {
    account_id: i64,
    token_digest: Vec<u8>,
    target: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    status: String,
}

#[derive(Clone, FromRow)]
struct MatchPersistenceRow {
    id: Uuid,
    result_commit_key: Uuid,
    mode: String,
    course_id: i64,
    hole: i16,
    hole_mode: i16,
    par: i16,
    catalog_sha256: Vec<u8>,
    seed: Vec<u8>,
    weather: String,
    wind_speed_tenths: i16,
    wind_angle_degrees: i16,
    reward_formula: String,
    status: String,
    account_id: i64,
    participant_order: i16,
    player_result_key: Uuid,
    strokes: Option<i16>,
    score: Option<i16>,
    pang_reward: Option<i64>,
    experience_reward: Option<i64>,
    pang_balance_after: Option<i64>,
    experience_balance_after: Option<i64>,
}

impl MatchPersistenceRow {
    fn matches_begin(&self, request: &BeginSoloMatch) -> Result<bool, MatchRepositoryError> {
        if self.mode != "solo_practice" || self.reward_formula != "solo-v1" {
            return Err(MatchRepositoryError::WrongMode);
        }
        if !(1..=18).contains(&self.hole)
            || self.participant_order != 0
            || self.player_result_key != self.result_commit_key
        {
            return Err(MatchRepositoryError::CorruptData);
        }
        let fingerprint = pangya_domain::CatalogFingerprint::from_slice(&self.catalog_sha256)
            .map_err(|_| MatchRepositoryError::CorruptData)?;
        let seed = pangya_domain::MatchSeed::from_slice(&self.seed)
            .map_err(|_| MatchRepositoryError::CorruptData)?;
        let weather = parse_weather(&self.weather)?;
        let wind = WindConditions::new(
            u16::try_from(self.wind_speed_tenths).map_err(|_| MatchRepositoryError::CorruptData)?,
            u16::try_from(self.wind_angle_degrees)
                .map_err(|_| MatchRepositoryError::CorruptData)?,
        )
        .map_err(|_| MatchRepositoryError::CorruptData)?;
        Ok(self.id == request.match_id().get()
            && self.result_commit_key == request.result_key().get()
            && self.account_id == request.account_id().get()
            && self.course_id == i64::from(request.config().course_id().get())
            && self.hole == i16::from(request.config().hole_count())
            && self.hole_mode == i16::from(request.config().hole_mode())
            && self.par == i16::from(request.config().par())
            && fingerprint == request.catalog_fingerprint()
            && seed == request.seed()
            && weather == request.weather()
            && wind == request.wind())
    }

    fn persisted_result(&self) -> Result<SoloMatchResult, MatchRepositoryError> {
        if self.status != "committed"
            || self.mode != "solo_practice"
            || !(1..=18).contains(&self.hole)
            || self.reward_formula != "solo-v1"
            || self.participant_order != 0
            || self.player_result_key != self.result_commit_key
        {
            return Err(MatchRepositoryError::CorruptData);
        }
        let strokes = self
            .strokes
            .and_then(|value| u16::try_from(value).ok())
            .ok_or(MatchRepositoryError::CorruptData)
            .and_then(|value| {
                StrokeCount::new(value).map_err(|_| MatchRepositoryError::CorruptData)
            })?;
        let reward = pangya_domain::SoloReward::from_persisted(
            self.score.ok_or(MatchRepositoryError::CorruptData)?,
            checked_match_u64(self.pang_reward.ok_or(MatchRepositoryError::CorruptData)?)?,
            checked_match_u64(
                self.experience_reward
                    .ok_or(MatchRepositoryError::CorruptData)?,
            )?,
        );
        let balances = pangya_domain::ServerBalances::from_persisted(
            checked_match_u64(
                self.pang_balance_after
                    .ok_or(MatchRepositoryError::CorruptData)?,
            )?,
            checked_match_u64(
                self.experience_balance_after
                    .ok_or(MatchRepositoryError::CorruptData)?,
            )?,
        );
        Ok(SoloMatchResult::new(
            MatchId::new(self.id),
            MatchResultKey::new(self.result_commit_key),
            AccountId::new(self.account_id).map_err(|_| MatchRepositoryError::CorruptData)?,
            strokes,
            reward,
            balances,
        ))
    }
}

#[derive(Clone, FromRow)]
struct StrokeMatchPersistenceRow {
    id: Uuid,
    result_commit_key: Uuid,
    mode: String,
    course_id: i64,
    hole: i16,
    hole_mode: i16,
    par: i16,
    catalog_sha256: Vec<u8>,
    seed: Vec<u8>,
    weather: String,
    wind_speed_tenths: i16,
    wind_angle_degrees: i16,
    reward_formula: String,
    status: String,
}

#[derive(Clone, FromRow)]
struct StrokePlayerPersistenceRow {
    account_id: i64,
    participant_order: i16,
    player_result_key: Uuid,
    strokes: Option<i16>,
    score: Option<i16>,
    quit: bool,
    place: Option<i16>,
    completion: Option<String>,
    pang_reward: Option<i64>,
    experience_reward: Option<i64>,
    pang_balance_after: Option<i64>,
    experience_balance_after: Option<i64>,
}

impl StrokeMatchPersistenceRow {
    fn matches_stroke_begin(
        &self,
        request: &BeginStrokeMatch,
        players: &[StrokePlayerPersistenceRow],
    ) -> Result<bool, MatchRepositoryError> {
        validate_stroke_aggregate(self, players, request.result_key())?;
        let fingerprint = pangya_domain::CatalogFingerprint::from_slice(&self.catalog_sha256)
            .map_err(|_| MatchRepositoryError::CorruptData)?;
        let seed = pangya_domain::MatchSeed::from_slice(&self.seed)
            .map_err(|_| MatchRepositoryError::CorruptData)?;
        let wind = WindConditions::new(
            u16::try_from(self.wind_speed_tenths).map_err(|_| MatchRepositoryError::CorruptData)?,
            u16::try_from(self.wind_angle_degrees)
                .map_err(|_| MatchRepositoryError::CorruptData)?,
        )
        .map_err(|_| MatchRepositoryError::CorruptData)?;
        Ok(self.id == request.match_id().get()
            && self.course_id == i64::from(request.config().course_id().get())
            && self.hole == i16::from(request.config().hole_count())
            && self.hole_mode == i16::from(request.config().hole_mode())
            && self.par == i16::from(request.config().par())
            && fingerprint == request.catalog_fingerprint()
            && seed == request.seed()
            && parse_weather(&self.weather)? == request.weather()
            && wind == request.wind()
            && players
                .iter()
                .zip(request.participants())
                .all(|(row, input)| {
                    row.account_id == input.account_id().get()
                        && row.participant_order == i16::from(input.roster_order().get())
                        && row.player_result_key == input.player_result_key().get()
                }))
    }
}

async fn lock_stroke_match(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
) -> Result<(StrokeMatchPersistenceRow, Vec<StrokePlayerPersistenceRow>), MatchRepositoryError> {
    let row = sqlx::query_as!(
        StrokeMatchPersistenceRow,
        r#"SELECT id AS "id!", result_commit_key AS "result_commit_key!", mode AS "mode!",
                  course_id AS "course_id!", hole AS "hole!", hole_mode AS "hole_mode!", par AS "par!",
                  catalog_sha256 AS "catalog_sha256!", seed AS "seed!", weather AS "weather!",
                  wind_speed_tenths AS "wind_speed_tenths!",
                  wind_angle_degrees AS "wind_angle_degrees!",
                  reward_formula AS "reward_formula!", status AS "status!"
           FROM matches WHERE id = $1 FOR UPDATE"#,
        match_id.get()
    )
    .fetch_optional(&mut **transaction)
    .await
    .map_err(match_db_error)?
    .ok_or(MatchRepositoryError::NotFound)?;
    let players = sqlx::query_as!(
        StrokePlayerPersistenceRow,
        r#"SELECT account_id AS "account_id!", participant_order AS "participant_order!",
                  player_result_key AS "player_result_key!", strokes AS "strokes?",
                  score AS "score?", quit AS "quit!", place AS "place?",
                  completion AS "completion?", pang_reward AS "pang_reward?",
                  experience_reward AS "experience_reward?",
                  pang_balance_after AS "pang_balance_after?",
                  experience_balance_after AS "experience_balance_after?"
           FROM match_players WHERE match_id = $1 ORDER BY participant_order FOR UPDATE"#,
        match_id.get()
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(match_db_error)?;
    Ok((row, players))
}

fn validate_stroke_aggregate(
    row: &StrokeMatchPersistenceRow,
    players: &[StrokePlayerPersistenceRow],
    result_key: MatchResultKey,
) -> Result<(), MatchRepositoryError> {
    if row.result_commit_key != result_key.get() {
        return Err(MatchRepositoryError::WrongResultKey);
    }
    let [first, second] = players else {
        return Err(MatchRepositoryError::CorruptData);
    };
    if row.mode != "stroke_two"
        || row.reward_formula != "stroke-two-v1"
        || !(1..=18).contains(&row.hole)
        || first.participant_order != 0
        || second.participant_order != 1
        || first.account_id == second.account_id
        || first.player_result_key == second.player_result_key
        || first.player_result_key == row.result_commit_key
        || second.player_result_key == row.result_commit_key
    {
        return Err(MatchRepositoryError::CorruptData);
    }
    Ok(())
}

fn validate_stroke_commit(
    row: &StrokeMatchPersistenceRow,
    persisted: &[StrokePlayerPersistenceRow],
    request: &CommitStrokeMatch,
) -> Result<(), MatchRepositoryError> {
    validate_stroke_aggregate(row, persisted, request.result_key())?;
    if row.course_id != i64::from(request.config().course_id().get())
        || row.hole != i16::from(request.config().hole_count())
        || row.hole_mode != i16::from(request.config().hole_mode())
        || row.par != i16::from(request.config().par())
    {
        return Err(MatchRepositoryError::WrongConfig);
    }
    if persisted.iter().zip(request.players()).any(|(row, input)| {
        row.account_id != input.participant().account_id().get()
            || row.participant_order != i16::from(input.participant().roster_order().get())
            || row.player_result_key != input.participant().player_result_key().get()
    }) {
        return Err(MatchRepositoryError::InputDrift);
    }
    Ok(())
}

fn persisted_stroke_result(
    row: &StrokeMatchPersistenceRow,
    players: &[StrokePlayerPersistenceRow],
) -> Result<StrokeMatchResult, MatchRepositoryError> {
    validate_stroke_aggregate(row, players, MatchResultKey::new(row.result_commit_key))?;
    if row.status != "committed" {
        return Err(MatchRepositoryError::CorruptData);
    }
    let config = pangya_domain::MatchPlan::with_holes(
        pangya_domain::CourseId::try_from(row.course_id)
            .map_err(|_| MatchRepositoryError::CorruptData)?,
        u8::try_from(row.hole).map_err(|_| MatchRepositoryError::CorruptData)?,
        u8::try_from(row.hole_mode).map_err(|_| MatchRepositoryError::CorruptData)?,
        u8::try_from(row.par).map_err(|_| MatchRepositoryError::CorruptData)?,
    )
    .map_err(|_| MatchRepositoryError::CorruptData)?;
    let mut results = Vec::with_capacity(2);
    for persisted in players {
        let participant = pangya_domain::StrokeParticipant::new(
            AccountId::new(persisted.account_id).map_err(|_| MatchRepositoryError::CorruptData)?,
            StrokeRosterOrder::from_persisted(persisted.participant_order)
                .map_err(|_| MatchRepositoryError::CorruptData)?,
            MatchResultKey::new(persisted.player_result_key),
        );
        let completion = parse_stroke_completion(
            persisted
                .completion
                .as_deref()
                .ok_or(MatchRepositoryError::CorruptData)?,
        )?;
        let input = StrokePlayerCommit::new(
            participant,
            persisted
                .strokes
                .and_then(|value| u16::try_from(value).ok())
                .ok_or(MatchRepositoryError::CorruptData)?,
            StrokePlace::from_persisted(persisted.place.ok_or(MatchRepositoryError::CorruptData)?)
                .map_err(|_| MatchRepositoryError::CorruptData)?,
            completion,
        )
        .map_err(|_| MatchRepositoryError::CorruptData)?;
        if persisted.quit != completion.is_forfeit() {
            return Err(MatchRepositoryError::CorruptData);
        }
        let reward = StrokeReward::from_persisted(
            persisted.score,
            checked_match_u64(
                persisted
                    .pang_reward
                    .ok_or(MatchRepositoryError::CorruptData)?,
            )?,
            checked_match_u64(
                persisted
                    .experience_reward
                    .ok_or(MatchRepositoryError::CorruptData)?,
            )?,
        );
        if reward
            != synthetic_stroke_reward_v1(config, input.strokes(), completion)
                .map_err(|_| MatchRepositoryError::CorruptData)?
        {
            return Err(MatchRepositoryError::CorruptData);
        }
        let balances = ServerBalances::from_persisted(
            checked_match_u64(
                persisted
                    .pang_balance_after
                    .ok_or(MatchRepositoryError::CorruptData)?,
            )?,
            checked_match_u64(
                persisted
                    .experience_balance_after
                    .ok_or(MatchRepositoryError::CorruptData)?,
            )?,
        );
        results.push(StrokePlayerResult::new(input, reward, balances));
    }
    let [first, second] = results.as_slice() else {
        return Err(MatchRepositoryError::CorruptData);
    };
    Ok(StrokeMatchResult::new(
        MatchId::new(row.id),
        MatchResultKey::new(row.result_commit_key),
        [*first, *second],
    ))
}

fn validate_stroke_replay(
    result: &StrokeMatchResult,
    request: &CommitStrokeMatch,
) -> Result<(), MatchRepositoryError> {
    if result.match_id() != request.match_id()
        || result.result_key() != request.result_key()
        || result
            .players()
            .iter()
            .zip(request.players())
            .any(|(persisted, input)| {
                persisted.participant() != input.participant()
                    || persisted.strokes() != input.strokes()
                    || persisted.place() != input.place()
                    || persisted.completion() != input.completion()
            })
    {
        return Err(MatchRepositoryError::InputDrift);
    }
    Ok(())
}

async fn lock_match(
    transaction: &mut Transaction<'_, Postgres>,
    match_id: MatchId,
) -> Result<MatchPersistenceRow, MatchRepositoryError> {
    sqlx::query_as!(
        MatchPersistenceRow,
        r#"SELECT m.id AS "id!", m.result_commit_key AS "result_commit_key!",
                  m.course_id AS "course_id!", m.hole AS "hole!", m.hole_mode AS "hole_mode!", m.par AS "par!",
                  m.catalog_sha256 AS "catalog_sha256!", m.seed AS "seed!",
                  m.weather AS "weather!",
                  m.wind_speed_tenths AS "wind_speed_tenths!",
                  m.wind_angle_degrees AS "wind_angle_degrees!",
                  m.mode AS "mode!", m.reward_formula AS "reward_formula!",
                  m.status AS "status!", mp.account_id AS "account_id!",
                  mp.participant_order AS "participant_order!",
                  mp.player_result_key AS "player_result_key!",
                  mp.strokes AS "strokes?", mp.score AS "score?",
                  mp.pang_reward AS "pang_reward?",
                  mp.experience_reward AS "experience_reward?",
                  mp.pang_balance_after AS "pang_balance_after?",
                  mp.experience_balance_after AS "experience_balance_after?"
           FROM matches m JOIN match_players mp ON mp.match_id = m.id
           WHERE m.id = $1 FOR UPDATE OF m, mp"#,
        match_id.get()
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(match_db_error)
    .and_then(|rows| {
        if rows.is_empty() {
            return Err(MatchRepositoryError::NotFound);
        }
        if rows.iter().any(|row| row.mode != "solo_practice") {
            return Err(MatchRepositoryError::WrongMode);
        }
        let [row] = rows.as_slice() else {
            return Err(MatchRepositoryError::CorruptData);
        };
        Ok(row.clone())
    })
}

fn validate_authority(
    row: &MatchPersistenceRow,
    account_id: AccountId,
    result_key: MatchResultKey,
) -> Result<(), MatchRepositoryError> {
    if row.mode != "solo_practice" || row.reward_formula != "solo-v1" {
        return Err(MatchRepositoryError::WrongMode);
    }
    if row.participant_order != 0 || row.player_result_key != row.result_commit_key {
        return Err(MatchRepositoryError::CorruptData);
    }
    if row.account_id != account_id.get() {
        return Err(MatchRepositoryError::WrongAccount);
    }
    if row.result_commit_key != result_key.get() || row.player_result_key != result_key.get() {
        return Err(MatchRepositoryError::WrongResultKey);
    }
    Ok(())
}

/// Adds an operator grant to a stored balance, refusing rather than wrapping.
fn operator_balance_add(current: i64, grant: u64) -> Result<i64, RepositoryError> {
    let current = u64::try_from(current).map_err(|_| RepositoryError::CorruptData)?;
    let balance = current
        .checked_add(grant)
        .ok_or(RepositoryError::BalanceOverflow)?;
    i64::try_from(balance).map_err(|_| RepositoryError::BalanceOverflow)
}

fn checked_balance_add(current: i64, reward: u64) -> Result<i64, MatchRepositoryError> {
    let current = u64::try_from(current).map_err(|_| MatchRepositoryError::CorruptData)?;
    let balance = current
        .checked_add(reward)
        .ok_or(MatchRepositoryError::BalanceOverflow)?;
    i64::try_from(balance).map_err(|_| MatchRepositoryError::BalanceOverflow)
}

fn checked_match_u64(value: i64) -> Result<u64, MatchRepositoryError> {
    u64::try_from(value).map_err(|_| MatchRepositoryError::CorruptData)
}

const fn weather_text(weather: Weather) -> &'static str {
    match weather {
        Weather::Clear => "clear",
        Weather::Cloudy => "cloudy",
        Weather::Rain => "rain",
    }
}

fn parse_weather(value: &str) -> Result<Weather, MatchRepositoryError> {
    match value {
        "clear" => Ok(Weather::Clear),
        "cloudy" => Ok(Weather::Cloudy),
        "rain" => Ok(Weather::Rain),
        _ => Err(MatchRepositoryError::CorruptData),
    }
}

const fn stroke_completion_text(completion: StrokeCompletion) -> &'static str {
    match completion {
        StrokeCompletion::Holed => "holed",
        StrokeCompletion::StrokeCap => "stroke_cap",
        StrokeCompletion::WinnerByForfeit => "winner_by_forfeit",
        StrokeCompletion::GiveUp => "give_up",
        StrokeCompletion::Disconnect => "disconnect",
        StrokeCompletion::TurnTimeout => "turn_timeout",
        StrokeCompletion::GameTimeout => "game_timeout",
    }
}

fn parse_stroke_completion(value: &str) -> Result<StrokeCompletion, MatchRepositoryError> {
    match value {
        "holed" => Ok(StrokeCompletion::Holed),
        "stroke_cap" => Ok(StrokeCompletion::StrokeCap),
        "winner_by_forfeit" => Ok(StrokeCompletion::WinnerByForfeit),
        "give_up" => Ok(StrokeCompletion::GiveUp),
        "disconnect" => Ok(StrokeCompletion::Disconnect),
        "turn_timeout" => Ok(StrokeCompletion::TurnTimeout),
        "game_timeout" => Ok(StrokeCompletion::GameTimeout),
        _ => Err(MatchRepositoryError::CorruptData),
    }
}

const fn abort_reason_text(reason: MatchAbortReason) -> &'static str {
    match reason {
        MatchAbortReason::Disconnect => "disconnect",
        MatchAbortReason::LoadingTimeout => "loading_timeout",
        MatchAbortReason::Shutdown => "shutdown",
        MatchAbortReason::StartupRecovery => "startup_recovery",
        MatchAbortReason::PersistenceFailure => "persistence_failure",
    }
}

async fn lock_active_account(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
) -> Result<(), RepositoryError> {
    let status = sqlx::query_scalar!(
        "SELECT status FROM accounts WHERE id = $1 FOR UPDATE",
        account_id.get()
    )
    .fetch_optional(&mut **transaction)
    .await
    .map_err(repository_db_error)?
    .ok_or(RepositoryError::NotFound)?;
    if parse_account_status(&status)? != AccountStatus::Active {
        return Err(RepositoryError::AccountInactive);
    }
    Ok(())
}

fn ensure_starter_bounds(grant: &StarterGrant) -> Result<(), RepositoryError> {
    if grant.items.len() > MAX_STARTER_ITEMS {
        return Err(RepositoryError::InvalidStarterGrant);
    }
    Ok(())
}

async fn apply_starter(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
    grant: &StarterGrant,
) -> Result<(), RepositoryError> {
    ensure_starter_bounds(grant)?;
    lock_active_account(transaction, account_id).await?;
    let mut configured_keys = HashSet::with_capacity(grant.items.len());
    if grant
        .items
        .iter()
        .any(|item| item.quantity == 0 || !configured_keys.insert(item.key.as_str()))
    {
        return Err(RepositoryError::InvalidStarterGrant);
    }
    for equipped_key in [
        grant.equipped_club_key.as_ref(),
        grant.equipped_ball_key.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        if !configured_keys.contains(equipped_key.as_str()) {
            return Err(RepositoryError::InvalidStarterGrant);
        }
    }

    let profile = sqlx::query!(
        "SELECT nickname_normalized, selected_character_id, setup_state \
         FROM profiles WHERE account_id = $1 FOR UPDATE",
        account_id.get()
    )
    .fetch_optional(&mut **transaction)
    .await
    .map_err(repository_db_error)?
    .ok_or(RepositoryError::NotFound)?;
    let characters = sqlx::query!(
        "SELECT id, item_type_id, starter_key FROM characters \
         WHERE account_id = $1 ORDER BY id FOR UPDATE",
        account_id.get()
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    let items = sqlx::query!(
        "SELECT id, item_type_id, starter_key, quantity FROM inventory_items \
         WHERE account_id = $1 ORDER BY id FOR UPDATE",
        account_id.get()
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    let equipment = sqlx::query!(
        "SELECT id, character_id, club_item_id, ball_item_id, version, updated_at \
         FROM equipment_sets WHERE account_id = $1 FOR UPDATE",
        account_id.get()
    )
    .fetch_optional(&mut **transaction)
    .await
    .map_err(repository_db_error)?;

    let has_existing_state = !characters.is_empty()
        || !items.is_empty()
        || equipment.is_some()
        || profile.selected_character_id.is_some();
    let expected_setup = if profile.nickname_normalized.is_some() {
        "complete"
    } else {
        "needs_nickname"
    };

    if has_existing_state {
        let [character] = characters.as_slice() else {
            return Err(RepositoryError::InvalidStarterGrant);
        };
        if character.item_type_id != i64::from(grant.character.item_type_id.get())
            || character.starter_key != grant.character.key.as_str()
            || profile.selected_character_id != Some(character.id)
            || profile.setup_state != expected_setup
            || items.len() != grant.items.len()
        {
            return Err(RepositoryError::InvalidStarterGrant);
        }
        let persisted_items: HashMap<&str, (i64, i64, i64)> = items
            .iter()
            .map(|item| {
                (
                    item.starter_key.as_str(),
                    (item.id, item.item_type_id, item.quantity),
                )
            })
            .collect();
        for configured in &grant.items {
            let Some((_, item_type_id, quantity)) = persisted_items.get(configured.key.as_str())
            else {
                return Err(RepositoryError::InvalidStarterGrant);
            };
            if *item_type_id != i64::from(configured.item_type_id.get())
                || *quantity != i64::from(configured.quantity)
            {
                return Err(RepositoryError::InvalidStarterGrant);
            }
        }
        let equipment = equipment.ok_or(RepositoryError::InvalidStarterGrant)?;
        let club_item_id =
            configured_equipment_id(&persisted_items, grant.equipped_club_key.as_ref())?;
        let ball_item_id =
            configured_equipment_id(&persisted_items, grant.equipped_ball_key.as_ref())?;
        if equipment.character_id != character.id
            || equipment.club_item_id != club_item_id
            || equipment.ball_item_id != ball_item_id
        {
            return Err(RepositoryError::InvalidStarterGrant);
        }
        return Ok(());
    }

    let character_id: i64 = sqlx::query_scalar!(
        "INSERT INTO characters (account_id, item_type_id, starter_key) \
         VALUES ($1, $2, $3) RETURNING id",
        account_id.get(),
        i64::from(grant.character.item_type_id.get()),
        grant.character.key.as_str()
    )
    .fetch_one(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    let mut inserted_items = HashMap::with_capacity(grant.items.len());
    for item in &grant.items {
        let item_id: i64 = sqlx::query_scalar!(
            "INSERT INTO inventory_items \
             (account_id, item_type_id, starter_key, quantity) VALUES ($1, $2, $3, $4) \
             RETURNING id",
            account_id.get(),
            i64::from(item.item_type_id.get()),
            item.key.as_str(),
            i64::from(item.quantity)
        )
        .fetch_one(&mut **transaction)
        .await
        .map_err(repository_db_error)?;
        inserted_items.insert(
            item.key.as_str(),
            (
                item_id,
                i64::from(item.item_type_id.get()),
                i64::from(item.quantity),
            ),
        );
    }
    let club_item_id = configured_equipment_id(&inserted_items, grant.equipped_club_key.as_ref())?;
    let ball_item_id = configured_equipment_id(&inserted_items, grant.equipped_ball_key.as_ref())?;
    sqlx::query!(
        "INSERT INTO equipment_sets \
         (account_id, character_id, club_item_id, ball_item_id) VALUES ($1, $2, $3, $4)",
        account_id.get(),
        character_id,
        club_item_id,
        ball_item_id
    )
    .execute(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    sqlx::query!(
        "UPDATE profiles SET selected_character_id = $2, setup_state = $3, updated_at = now() \
         WHERE account_id = $1",
        account_id.get(),
        character_id,
        expected_setup
    )
    .execute(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    Ok(())
}

fn configured_equipment_id(
    items: &HashMap<&str, (i64, i64, i64)>,
    key: Option<&StarterKey>,
) -> Result<Option<i64>, RepositoryError> {
    key.map(|key| {
        items
            .get(key.as_str())
            .map(|(id, _, _)| *id)
            .ok_or(RepositoryError::InvalidStarterGrant)
    })
    .transpose()
}

fn player_snapshot_from_rows(
    requested: AccountId,
    account: AccountRow,
    profile: PlayerProfileRow,
    character_rows: Vec<CharacterRow>,
    inventory_rows: Vec<InventoryRow>,
    equipment: EquipmentRow,
) -> Result<PlayerSnapshot, RepositoryError> {
    let account_id = AccountId::new(account.id).map_err(|_| RepositoryError::CorruptData)?;
    let status = parse_account_status(&account.status)?;
    if account_id != requested
        || AccountId::new(profile.account_id).map_err(|_| RepositoryError::CorruptData)?
            != requested
        || AccountId::new(equipment.account_id).map_err(|_| RepositoryError::CorruptData)?
            != requested
    {
        return Err(RepositoryError::CorruptData);
    }
    if status != AccountStatus::Active {
        return Err(RepositoryError::AccountInactive);
    }
    if parse_setup_state(&profile.setup_state)? != SetupState::Complete
        || profile.nickname_display.is_none()
        || character_rows.is_empty()
        || character_rows.len() > MAX_PLAYER_CHARACTERS
        || inventory_rows.len() > MAX_PLAYER_INVENTORY
    {
        return Err(RepositoryError::CorruptData);
    }
    let selected_id = profile
        .selected_character_id
        .ok_or(RepositoryError::CorruptData)
        .and_then(|value| CharacterId::new(value).map_err(|_| RepositoryError::CorruptData))?;
    let mut character_ids = BTreeSet::new();
    let characters = character_rows
        .into_iter()
        .map(|row| {
            let owner = AccountId::new(row.account_id).map_err(|_| RepositoryError::CorruptData)?;
            let id = CharacterId::new(row.id).map_err(|_| RepositoryError::CorruptData)?;
            if owner != requested || !character_ids.insert(id) {
                return Err(RepositoryError::CorruptData);
            }
            Ok(Character {
                id,
                account_id: owner,
                item_type_id: ItemTypeId::try_from(row.item_type_id)
                    .map_err(|_| RepositoryError::CorruptData)?,
                starter_key: StarterKey::parse(&row.starter_key)
                    .map_err(|_| RepositoryError::CorruptData)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut inventory_ids = BTreeSet::new();
    let inventory = inventory_rows
        .into_iter()
        .map(|row| {
            let item = inventory_row_into_domain(row)?;
            if item.account_id != requested || !inventory_ids.insert(item.id) {
                return Err(RepositoryError::CorruptData);
            }
            Ok(item)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let equipment_character =
        CharacterId::new(equipment.character_id).map_err(|_| RepositoryError::CorruptData)?;
    let club_item_id = equipment
        .club_item_id
        .map(InventoryItemId::new)
        .transpose()
        .map_err(|_| RepositoryError::CorruptData)?;
    let ball_item_id = equipment
        .ball_item_id
        .map(InventoryItemId::new)
        .transpose()
        .map_err(|_| RepositoryError::CorruptData)?;
    if selected_id != equipment_character
        || !character_ids.contains(&selected_id)
        || club_item_id.is_some_and(|id| !inventory_ids.contains(&id))
        || ball_item_id.is_some_and(|id| !inventory_ids.contains(&id))
    {
        return Err(RepositoryError::CorruptData);
    }
    Ok(PlayerSnapshot {
        account: Account {
            id: account_id,
            username_display: account.username_display,
            username_normalized: NormalizedUsername::parse(&account.username_normalized)
                .map_err(|_| RepositoryError::CorruptData)?,
            status,
        },
        profile: Profile {
            account_id: requested,
            nickname: profile.nickname_display,
            setup_state: SetupState::Complete,
            pang: checked_u64(profile.pang)?,
            points: checked_u64(profile.points)?,
            experience: checked_u64(profile.experience)?,
        },
        characters,
        inventory,
        equipment: EquipmentSet {
            id: EquipmentSetId::new(equipment.id).map_err(|_| RepositoryError::CorruptData)?,
            account_id: requested,
            character_id: equipment_character,
            club_item_id,
            ball_item_id,
            version: u32::try_from(equipment.version).map_err(|_| RepositoryError::CorruptData)?,
        },
    })
}

async fn load_aggregate_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
) -> Result<AccountAggregate, RepositoryError> {
    let account = sqlx::query_as!(
        AccountRow,
        "SELECT id, username_display, username_normalized, status FROM accounts WHERE id = $1",
        account_id.get()
    )
    .fetch_optional(&mut **transaction)
    .await
    .map_err(repository_db_error)?
    .ok_or(RepositoryError::NotFound)?;
    let profile = sqlx::query_as!(
        ProfileRow,
        "SELECT account_id, nickname_display, setup_state, pang, points, experience \
         FROM profiles WHERE account_id = $1",
        account_id.get()
    )
    .fetch_one(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    let character = sqlx::query_as!(
        CharacterRow,
        "SELECT c.id, c.account_id, c.item_type_id, c.starter_key \
         FROM characters c JOIN profiles p ON p.selected_character_id = c.id \
         WHERE p.account_id = $1",
        account_id.get()
    )
    .fetch_one(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    let inventory = sqlx::query_as!(
        InventoryRow,
        "SELECT id, account_id, item_type_id, quantity, starter_key, inventory_class, \
                durability, expires_at \
         FROM inventory_items WHERE account_id = $1 ORDER BY id",
        account_id.get()
    )
    .fetch_all(&mut **transaction)
    .await
    .map_err(repository_db_error)?;
    let equipment = sqlx::query_as!(
        EquipmentRow,
        "SELECT id, account_id, character_id, club_item_id, ball_item_id, version \
         FROM equipment_sets WHERE account_id = $1",
        account_id.get()
    )
    .fetch_one(&mut **transaction)
    .await
    .map_err(repository_db_error)?;

    Ok(AccountAggregate {
        account: Account {
            id: AccountId::new(account.id).map_err(|_| RepositoryError::CorruptData)?,
            username_display: account.username_display,
            username_normalized: NormalizedUsername::parse(&account.username_normalized)
                .map_err(|_| RepositoryError::CorruptData)?,
            status: parse_account_status(&account.status)?,
        },
        profile: Profile {
            account_id: AccountId::new(profile.account_id)
                .map_err(|_| RepositoryError::CorruptData)?,
            nickname: profile.nickname_display,
            setup_state: parse_setup_state(&profile.setup_state)?,
            pang: checked_u64(profile.pang)?,
            points: checked_u64(profile.points)?,
            experience: checked_u64(profile.experience)?,
        },
        character: Character {
            id: CharacterId::new(character.id).map_err(|_| RepositoryError::CorruptData)?,
            account_id: AccountId::new(character.account_id)
                .map_err(|_| RepositoryError::CorruptData)?,
            item_type_id: ItemTypeId::try_from(character.item_type_id)
                .map_err(|_| RepositoryError::CorruptData)?,
            starter_key: StarterKey::parse(&character.starter_key)
                .map_err(|_| RepositoryError::CorruptData)?,
        },
        inventory: inventory
            .into_iter()
            .map(inventory_row_into_domain)
            .collect::<Result<Vec<_>, _>>()?,
        equipment: EquipmentSet {
            id: EquipmentSetId::new(equipment.id).map_err(|_| RepositoryError::CorruptData)?,
            account_id: AccountId::new(equipment.account_id)
                .map_err(|_| RepositoryError::CorruptData)?,
            character_id: CharacterId::new(equipment.character_id)
                .map_err(|_| RepositoryError::CorruptData)?,
            club_item_id: equipment
                .club_item_id
                .map(InventoryItemId::new)
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            ball_item_id: equipment
                .ball_item_id
                .map(InventoryItemId::new)
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            version: u32::try_from(equipment.version).map_err(|_| RepositoryError::CorruptData)?,
        },
    })
}

fn inventory_row_into_domain(row: InventoryRow) -> Result<InventoryItem, RepositoryError> {
    Ok(InventoryItem {
        id: InventoryItemId::new(row.id).map_err(|_| RepositoryError::CorruptData)?,
        account_id: AccountId::new(row.account_id).map_err(|_| RepositoryError::CorruptData)?,
        item_type_id: ItemTypeId::try_from(row.item_type_id)
            .map_err(|_| RepositoryError::CorruptData)?,
        quantity: u32::try_from(row.quantity).map_err(|_| RepositoryError::CorruptData)?,
        starter_key: StarterKey::parse(&row.starter_key)
            .map_err(|_| RepositoryError::CorruptData)?,
        class: parse_inventory_class(&row.inventory_class)?,
        durability: match row.durability {
            Some(value) => InventoryDurability::Durable(
                u32::try_from(value).map_err(|_| RepositoryError::CorruptData)?,
            ),
            None => InventoryDurability::Nondurable,
        },
        expires_at: row.expires_at.map(SystemTime::from),
    })
}

fn parse_inventory_class(value: &str) -> Result<InventoryClass, RepositoryError> {
    match value {
        "legacy" => Ok(InventoryClass::Legacy),
        "club_set" => Ok(InventoryClass::ClubSet),
        "ball" => Ok(InventoryClass::Ball),
        "consumable" => Ok(InventoryClass::Consumable),
        "character_part" => Ok(InventoryClass::CharacterPart),
        "caddie" => Ok(InventoryClass::Caddie),
        "caddie_item" => Ok(InventoryClass::CaddieItem),
        "mascot" => Ok(InventoryClass::Mascot),
        "card" => Ok(InventoryClass::Card),
        "furniture" => Ok(InventoryClass::Furniture),
        "skin" => Ok(InventoryClass::Skin),
        "hair_style" => Ok(InventoryClass::HairStyle),
        "set_item" => Ok(InventoryClass::SetItem),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn checked_u64(value: i64) -> Result<u64, RepositoryError> {
    u64::try_from(value).map_err(|_| RepositoryError::CorruptData)
}

fn system_time(value: SystemTime) -> DateTime<Utc> {
    value.into()
}

fn account_status_text(value: AccountStatus) -> &'static str {
    match value {
        AccountStatus::Active => "active",
        AccountStatus::Banned => "banned",
        AccountStatus::Disabled => "disabled",
    }
}

fn parse_account_status(value: &str) -> Result<AccountStatus, RepositoryError> {
    match value {
        "active" => Ok(AccountStatus::Active),
        "banned" => Ok(AccountStatus::Banned),
        "disabled" => Ok(AccountStatus::Disabled),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn parse_setup_state(value: &str) -> Result<SetupState, RepositoryError> {
    match value {
        "needs_nickname" => Ok(SetupState::NeedsNickname),
        "needs_starter" => Ok(SetupState::NeedsStarter),
        "complete" => Ok(SetupState::Complete),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn service_kind_text(value: ServiceKind) -> &'static str {
    match value {
        ServiceKind::Game => "game",
        ServiceKind::Message => "message",
    }
}

fn parse_service_kind(value: &str) -> Result<ServiceKind, RepositoryError> {
    match value {
        "game" => Ok(ServiceKind::Game),
        "message" => Ok(ServiceKind::Message),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn repository_db_error(error: sqlx::Error) -> RepositoryError {
    let constraint = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::constraint);
    match constraint {
        Some("uq_accounts_username_normalized") => RepositoryError::DuplicateUsername,
        Some("uq_profiles_nickname_normalized") => RepositoryError::DuplicateNickname,
        Some("uq_characters_starter_key" | "uq_inventory_starter_key") => {
            RepositoryError::InvalidStarterGrant
        }
        _ => RepositoryError::Storage(storage_fault(&error)),
    }
}

/// Classifies a driver failure into a bounded, nonsensitive fault.
///
/// Only the `SQLSTATE` and the driver's own error kind are read. Server message text,
/// statement text, bound parameters, and row values are never inspected, so the result
/// cannot carry caller data into a log line, a metric label, or a returned error.
fn storage_fault(error: &sqlx::Error) -> StorageFault {
    if let Some(database_error) = error.as_database_error() {
        return database_error.code().map_or(StorageFault::Other, |code| {
            StorageFault::from_sqlstate(&code)
        });
    }
    match error {
        sqlx::Error::PoolTimedOut => StorageFault::PoolTimedOut,
        sqlx::Error::PoolClosed => StorageFault::PoolClosed,
        sqlx::Error::Io(_) | sqlx::Error::Tls(_) => StorageFault::Io,
        sqlx::Error::ColumnDecode { .. }
        | sqlx::Error::Decode(_)
        | sqlx::Error::ColumnNotFound(_)
        | sqlx::Error::ColumnIndexOutOfBounds { .. }
        | sqlx::Error::TypeNotFound { .. } => StorageFault::Decode,
        sqlx::Error::Protocol(_) => StorageFault::DriverProtocol,
        _ => StorageFault::Other,
    }
}

fn handover_db_error(error: sqlx::Error) -> HandoverError {
    HandoverError::Storage(storage_fault(&error))
}

fn match_db_error(error: sqlx::Error) -> MatchRepositoryError {
    MatchRepositoryError::Storage(storage_fault(&error))
}

fn stroke_begin_db_error(error: sqlx::Error) -> MatchRepositoryError {
    if error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| code == "23505")
    {
        MatchRepositoryError::InputDrift
    } else {
        MatchRepositoryError::Storage(storage_fault(&error))
    }
}

/// Marker retained for the M1 crate-boundary test.
#[must_use]
pub const fn crate_boundary() -> &'static str {
    "storage"
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn config_debug_redacts_url() {
        let config = PgStorageConfig::new("postgres://user:secret@example/database");
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn config_bounds_are_nonzero_by_default() {
        let config = PgStorageConfig::new("postgres://localhost/db");
        assert_eq!(
            NonZeroU32::new(config.max_connections).map(NonZeroU32::get),
            Some(10)
        );
    }

    #[sqlx::test(migrator = "MIGRATOR")]
    async fn repeatable_read_snapshot_keeps_one_generation_across_later_projections(pool: PgPool) {
        let repository = PgRepository::new(pool.clone());
        let aggregate = repository
            .create_account_inner(
                NewAccount {
                    username: pangya_domain::Username::parse("SnapshotBarrier").expect("username"),
                    credential_hash: CredentialHash::new("synthetic-test-hash".to_owned()),
                    nickname: Some(Nickname::parse("SnapshotNick").expect("nickname")),
                    starter: StarterGrant {
                        character: pangya_domain::StarterCharacter {
                            key: StarterKey::parse("snapshot.character").expect("character key"),
                            item_type_id: ItemTypeId::new(0x0400_0000),
                        },
                        items: vec![pangya_domain::StarterItem {
                            key: StarterKey::parse("snapshot.club").expect("item key"),
                            item_type_id: ItemTypeId::new(0x1000_0000),
                            quantity: 1,
                        }],
                        equipped_club_key: Some(
                            StarterKey::parse("snapshot.club").expect("equipment key"),
                        ),
                        equipped_ball_key: None,
                    },
                },
                false,
            )
            .await
            .expect("account");
        let account_id = aggregate.account.id;
        let (snapshot_started_tx, snapshot_started_rx) = tokio::sync::oneshot::channel();
        let (mutation_committed_tx, mutation_committed_rx) = tokio::sync::oneshot::channel();
        let loading_repository = repository.clone();
        let loading = tokio::spawn(async move {
            loading_repository
                .load_player_snapshot_with_checkpoint(account_id, async move {
                    let _ = snapshot_started_tx.send(());
                    let _ = mutation_committed_rx.await;
                })
                .await
        });

        snapshot_started_rx.await.expect("snapshot established");
        let mut mutation = pool.begin().await.expect("mutation transaction");
        sqlx::query!(
            "UPDATE profiles SET pang = 123 WHERE account_id = $1",
            account_id.get()
        )
        .execute(&mut *mutation)
        .await
        .expect("profile mutation");
        sqlx::query!(
            "UPDATE equipment_sets SET version = 1 WHERE account_id = $1",
            account_id.get()
        )
        .execute(&mut *mutation)
        .await
        .expect("equipment mutation");
        mutation.commit().await.expect("mutation commit");
        let _ = mutation_committed_tx.send(());

        let during = loading.await.expect("snapshot task").expect("snapshot");
        assert_eq!((during.profile.pang, during.equipment.version), (0, 0));
        let after = repository
            .load_player_snapshot(account_id)
            .await
            .expect("later snapshot");
        assert_eq!((after.profile.pang, after.equipment.version), (123, 1));
    }
}
