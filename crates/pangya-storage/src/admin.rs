//! PostgreSQL implementation of the operator admin surface boundary.
//!
//! Two properties are load-bearing here and are enforced in SQL rather than in the caller.
//!
//! *Authorisation is resolved per request, not per session.* `resolve_admin_session` joins
//! `accounts` and re-reads `role` and `status` every time. Demoting or banning an account
//! therefore takes effect on its next request, without waiting for a session to expire.
//!
//! *Losing authority revokes outstanding sessions.* `set_account_role` revokes in the same
//! transaction that demotes, so a stolen cookie cannot outlive the authority it was issued
//! under.

use super::*;
use pangya_domain::{
    AccountRole, AdminAccountDetail, AdminAccountQuery, AdminAccountSort, AdminAccountSummary,
    AdminAuditEvent, AdminAuthenticationRecord, AdminEquipmentUpdate, AdminItemGrant,
    AdminItemUpdate, AdminLeaderboardEntry, AdminLedgerEntry, AdminLedgerSource, AdminMatchEntry,
    AdminMutationError, AdminPage, AdminRepository, AdminSession, AdminSessionId,
    BalanceAssignment, CourseId, NewAdminAuditEvent, NewAdminSession, ResolveAdminSession,
    ShopOverlay, ShopOverride,
};

impl PgRepository {
    async fn load_admin_authentication_inner(
        &self,
        username: &NormalizedUsername,
    ) -> Result<Option<AdminAuthenticationRecord>, RepositoryError> {
        let row = sqlx::query!(
            "SELECT a.id, a.username_display, a.status, a.role, c.password_hash \
             FROM accounts a JOIN credentials c ON c.account_id = a.id \
             WHERE a.username_normalized = $1",
            username.as_str()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_db_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(AdminAuthenticationRecord {
            account_id: AccountId::new(row.id).map_err(|_| RepositoryError::CorruptData)?,
            username_display: row.username_display,
            credential_hash: CredentialHash::new(row.password_hash),
            status: parse_account_status(&row.status)?,
            role: AccountRole::parse(&row.role).map_err(|_| RepositoryError::CorruptData)?,
        }))
    }

    async fn issue_admin_session_inner(
        &self,
        request: NewAdminSession,
    ) -> Result<(), RepositoryError> {
        if request.expires_at <= request.issued_at {
            return Err(RepositoryError::CorruptData);
        }
        let issued_at = system_time(request.issued_at);
        let expires_at = system_time(request.expires_at);
        // Re-checked inside the insert rather than before it: the role could have been revoked
        // between the credential verification and this write.
        let inserted = sqlx::query_scalar!(
            "INSERT INTO admin_sessions \
             (id, account_id, token_digest, source_address_prefix, issued_at, expires_at, \
              last_seen_at) \
             SELECT $1, id, $3, $4, $5, $6, $5 FROM accounts \
             WHERE id = $2 AND role = 'admin' AND status = 'active' \
             RETURNING id",
            request.id.get(),
            request.account_id.get(),
            request.digest.as_bytes().as_slice(),
            request.source_address_prefix.as_str(),
            issued_at,
            expires_at
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_db_error)?;
        inserted.map(|_| ()).ok_or(RepositoryError::AccountInactive)
    }

    async fn resolve_admin_session_inner(
        &self,
        request: ResolveAdminSession,
    ) -> Result<Option<AdminSession>, RepositoryError> {
        let now = system_time(request.now);
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let row = sqlx::query!(
            "SELECT s.token_digest, s.expires_at, a.id AS account_id, a.username_display, \
                    a.status, a.role \
             FROM admin_sessions s JOIN accounts a ON a.id = s.account_id \
             WHERE s.id = $1 FOR UPDATE OF s",
            request.id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let Some(row) = row else {
            transaction.commit().await.map_err(repository_db_error)?;
            return Ok(None);
        };
        let stored = HandoverDigest::from_slice(&row.token_digest)
            .map_err(|_| RepositoryError::CorruptData)?;
        // Constant-time, so a wrong-but-existing selector cannot be turned into a digest oracle.
        let matches = bool::from(stored.as_bytes().ct_eq(request.digest.as_bytes()));
        let role = AccountRole::parse(&row.role).map_err(|_| RepositoryError::CorruptData)?;
        let status = parse_account_status(&row.status)?;
        if !matches
            || row.expires_at <= now
            || role != AccountRole::Admin
            || status != AccountStatus::Active
        {
            transaction.commit().await.map_err(repository_db_error)?;
            return Ok(None);
        }
        let refreshed = sqlx::query_scalar!(
            "UPDATE admin_sessions SET last_seen_at = $2 \
             WHERE id = $1 AND revoked_at IS NULL AND expires_at > $2 \
             RETURNING expires_at",
            request.id.get(),
            now
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let Some(expires_at) = refreshed else {
            transaction.commit().await.map_err(repository_db_error)?;
            return Ok(None);
        };
        transaction.commit().await.map_err(repository_db_error)?;
        let account_id =
            AccountId::new(row.account_id).map_err(|_| RepositoryError::CorruptData)?;
        Ok(Some(AdminSession {
            id: request.id,
            account_id,
            username_display: row.username_display,
            role,
            expires_at: expires_at.into(),
        }))
    }

    async fn revoke_admin_session_inner(
        &self,
        id: AdminSessionId,
        now: SystemTime,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            "UPDATE admin_sessions SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL",
            id.get(),
            system_time(now)
        )
        .execute(&self.pool)
        .await
        .map_err(repository_db_error)?;
        Ok(())
    }

    async fn revoke_admin_sessions_for_account_inner(
        &self,
        account_id: AccountId,
        now: SystemTime,
    ) -> Result<(), RepositoryError> {
        sqlx::query!(
            "UPDATE admin_sessions SET revoked_at = $2 \
             WHERE account_id = $1 AND revoked_at IS NULL",
            account_id.get(),
            system_time(now)
        )
        .execute(&self.pool)
        .await
        .map_err(repository_db_error)?;
        Ok(())
    }

    async fn set_account_role_inner(
        &self,
        account_id: AccountId,
        role: AccountRole,
        now: SystemTime,
    ) -> Result<(), RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        let updated = sqlx::query_scalar!(
            "UPDATE accounts SET role = $2, updated_at = now() WHERE id = $1 RETURNING id",
            account_id.get(),
            role.as_str()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        if updated.is_none() {
            return Err(RepositoryError::NotFound);
        }
        // Demotion withdraws authority now rather than at the next expiry. Promotion revokes
        // too: a session issued before the promotion carries no role claim worth preserving,
        // and re-signing in is cheap.
        sqlx::query!(
            "UPDATE admin_sessions SET revoked_at = $2 \
             WHERE account_id = $1 AND revoked_at IS NULL",
            account_id.get(),
            system_time(now)
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(())
    }

    async fn record_admin_audit_inner(
        &self,
        event: NewAdminAuditEvent,
    ) -> Result<(), RepositoryError> {
        // Bound as text and cast, so this crate needs neither a JSON dependency nor sqlx's
        // `json` feature. `ck_admin_audit_detail_object` still rejects a non-object, and the
        // caller is expected to have serialised the value rather than hand-written it.
        sqlx::query!(
            "INSERT INTO admin_audit_events \
             (actor_account_id, action, target_account_id, detail) \
             VALUES ($1, $2, $3, ($4::text)::jsonb)",
            event.actor_account_id.get(),
            event.action,
            event.target_account_id.map(AccountId::get),
            event.detail
        )
        .execute(&self.pool)
        .await
        .map_err(repository_db_error)?;
        Ok(())
    }

    async fn list_admin_audit_inner(
        &self,
        page: AdminPage,
    ) -> Result<Vec<AdminAuditEvent>, RepositoryError> {
        let rows = sqlx::query!(
            "SELECT e.id, e.actor_account_id, a.username_display, e.action, \
                    e.target_account_id, e.detail::text AS \"detail!\", e.occurred_at \
             FROM admin_audit_events e JOIN accounts a ON a.id = e.actor_account_id \
             ORDER BY e.occurred_at DESC, e.id DESC LIMIT $1 OFFSET $2",
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(AdminAuditEvent {
                    id: row.id,
                    actor_account_id: AccountId::new(row.actor_account_id)
                        .map_err(|_| RepositoryError::CorruptData)?,
                    actor_username: row.username_display,
                    action: row.action,
                    target_account_id: row
                        .target_account_id
                        .map(AccountId::new)
                        .transpose()
                        .map_err(|_| RepositoryError::CorruptData)?,
                    detail: row.detail,
                    occurred_at: row.occurred_at.into(),
                })
            })
            .collect()
    }
}

impl PgRepository {
    async fn list_accounts_inner(
        &self,
        query: AdminAccountQuery,
    ) -> Result<Vec<AdminAccountSummary>, RepositoryError> {
        // One statement with every filter expressed as `$n IS NULL OR …`, and ordering as a
        // CASE over a closed enum. That keeps this on the checked `query!` macro: no operator
        // input is ever concatenated into SQL, and the column list stays type-verified.
        let search = query.search.as_ref().map(|value| format!("%{value}%"));
        let status = query.status.map(account_status_text);
        let role = query.role.map(AccountRole::as_str);
        let sort = match query.sort {
            AdminAccountSort::CreatedDesc => 0_i32,
            AdminAccountSort::CreatedAsc => 1,
            AdminAccountSort::PangDesc => 2,
            AdminAccountSort::ExperienceDesc => 3,
            AdminAccountSort::UsernameAsc => 4,
        };
        let rows = sqlx::query!(
            r#"SELECT a.id, a.username_display, a.status, a.role, a.created_at,
                      p.nickname_display, p.setup_state, p.rank, p.experience, p.pang, p.points,
                      (SELECT count(*) FROM characters c WHERE c.account_id = a.id)
                        AS "character_count!",
                      (SELECT count(*) FROM inventory_items i WHERE i.account_id = a.id)
                        AS "inventory_count!"
               FROM accounts a JOIN profiles p ON p.account_id = a.id
               WHERE ($1::text IS NULL
                      OR a.username_normalized ILIKE $1
                      OR p.nickname_normalized ILIKE $1)
                 AND ($2::text IS NULL OR a.status = $2)
                 AND ($3::text IS NULL OR a.role = $3)
               ORDER BY
                 CASE WHEN $4 = 0 THEN a.created_at END DESC,
                 CASE WHEN $4 = 1 THEN a.created_at END ASC,
                 CASE WHEN $4 = 2 THEN p.pang END DESC,
                 CASE WHEN $4 = 3 THEN p.experience END DESC,
                 CASE WHEN $4 = 4 THEN a.username_normalized END ASC,
                 a.id DESC
               LIMIT $5 OFFSET $6"#,
            search,
            status,
            role,
            sort,
            query.page.limit(),
            query.page.offset()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(AdminAccountSummary {
                    id: AccountId::new(row.id).map_err(|_| RepositoryError::CorruptData)?,
                    username: row.username_display,
                    nickname: row.nickname_display,
                    status: parse_account_status(&row.status)?,
                    role: AccountRole::parse(&row.role)
                        .map_err(|_| RepositoryError::CorruptData)?,
                    setup_state: parse_setup_state(&row.setup_state)?,
                    rank: u32::try_from(row.rank).map_err(|_| RepositoryError::CorruptData)?,
                    experience: checked_u64(row.experience)?,
                    pang: checked_u64(row.pang)?,
                    points: checked_u64(row.points)?,
                    character_count: row.character_count,
                    inventory_count: row.inventory_count,
                    created_at: row.created_at.into(),
                })
            })
            .collect()
    }

    async fn load_account_detail_inner(
        &self,
        account_id: AccountId,
    ) -> Result<AdminAccountDetail, RepositoryError> {
        let summary = sqlx::query!(
            r#"SELECT a.id, a.username_display, a.status, a.role, a.created_at,
                      p.nickname_display, p.setup_state, p.rank, p.experience, p.pang, p.points,
                      p.selected_character_id
               FROM accounts a JOIN profiles p ON p.account_id = a.id
               WHERE a.id = $1"#,
            account_id.get()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_db_error)?
        .ok_or(RepositoryError::NotFound)?;

        let characters = sqlx::query!(
            "SELECT id, account_id, item_type_id, starter_key FROM characters \
             WHERE account_id = $1 ORDER BY id",
            account_id.get()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;

        let inventory = sqlx::query_as!(
            InventoryRow,
            "SELECT id, account_id, item_type_id, quantity, starter_key, inventory_class, \
                    durability, expires_at \
             FROM inventory_items WHERE account_id = $1 ORDER BY id",
            account_id.get()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;

        let equipment = sqlx::query!(
            "SELECT id, account_id, character_id, club_item_id, ball_item_id, version \
             FROM equipment_sets WHERE account_id = $1",
            account_id.get()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_db_error)?;

        Ok(AdminAccountDetail {
            summary: AdminAccountSummary {
                id: AccountId::new(summary.id).map_err(|_| RepositoryError::CorruptData)?,
                username: summary.username_display,
                nickname: summary.nickname_display,
                status: parse_account_status(&summary.status)?,
                role: AccountRole::parse(&summary.role)
                    .map_err(|_| RepositoryError::CorruptData)?,
                setup_state: parse_setup_state(&summary.setup_state)?,
                rank: u32::try_from(summary.rank).map_err(|_| RepositoryError::CorruptData)?,
                experience: checked_u64(summary.experience)?,
                pang: checked_u64(summary.pang)?,
                points: checked_u64(summary.points)?,
                character_count: i64::try_from(characters.len())
                    .map_err(|_| RepositoryError::CorruptData)?,
                inventory_count: i64::try_from(inventory.len())
                    .map_err(|_| RepositoryError::CorruptData)?,
                created_at: summary.created_at.into(),
            },
            characters: characters
                .into_iter()
                .map(|row| {
                    Ok(Character {
                        id: CharacterId::new(row.id).map_err(|_| RepositoryError::CorruptData)?,
                        account_id: AccountId::new(row.account_id)
                            .map_err(|_| RepositoryError::CorruptData)?,
                        item_type_id: ItemTypeId::try_from(row.item_type_id)
                            .map_err(|_| RepositoryError::CorruptData)?,
                        starter_key: StarterKey::parse(&row.starter_key)
                            .map_err(|_| RepositoryError::CorruptData)?,
                    })
                })
                .collect::<Result<Vec<_>, RepositoryError>>()?,
            inventory: inventory
                .into_iter()
                .map(inventory_row_into_domain)
                .collect::<Result<Vec<_>, RepositoryError>>()?,
            equipment: equipment
                .map(|row| {
                    Ok::<_, RepositoryError>(EquipmentSet {
                        id: EquipmentSetId::new(row.id)
                            .map_err(|_| RepositoryError::CorruptData)?,
                        account_id: AccountId::new(row.account_id)
                            .map_err(|_| RepositoryError::CorruptData)?,
                        character_id: CharacterId::new(row.character_id)
                            .map_err(|_| RepositoryError::CorruptData)?,
                        club_item_id: row
                            .club_item_id
                            .map(InventoryItemId::new)
                            .transpose()
                            .map_err(|_| RepositoryError::CorruptData)?,
                        ball_item_id: row
                            .ball_item_id
                            .map(InventoryItemId::new)
                            .transpose()
                            .map_err(|_| RepositoryError::CorruptData)?,
                        version: u32::try_from(row.version)
                            .map_err(|_| RepositoryError::CorruptData)?,
                    })
                })
                .transpose()?,
            selected_character_id: summary
                .selected_character_id
                .map(CharacterId::new)
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
        })
    }

    async fn list_account_ledger_inner(
        &self,
        account_id: AccountId,
        page: AdminPage,
    ) -> Result<Vec<AdminLedgerEntry>, RepositoryError> {
        // Four tables with different shapes and different authority triggers, merged into one
        // ordered answer because "where did this player's pang go" is one question.
        let rows = sqlx::query!(
            r#"SELECT source AS "source!", delta AS "delta!", balance_after,
                      reason AS "reason!", reference AS "reference!",
                      created_at AS "created_at!"
               FROM (
                 SELECT 'match_pang' AS source, delta, balance_after, reason,
                        match_id::text AS reference, created_at
                   FROM currency_ledger WHERE account_id = $1
                 UNION ALL
                 SELECT 'match_experience', delta, balance_after, reason,
                        match_id::text, created_at
                   FROM progression_ledger WHERE account_id = $1
                 UNION ALL
                 SELECT 'shop_pang', delta, balance_after, reason,
                        operation_id::text, created_at
                   FROM shop_currency_ledger WHERE account_id = $1
                 UNION ALL
                 SELECT 'item', quantity_delta, quantity_after, reason,
                        inventory_id::text, created_at
                   FROM item_ledger WHERE account_id = $1
               ) merged
               ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
            account_id.get(),
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(AdminLedgerEntry {
                    source: match row.source.as_str() {
                        "match_pang" => AdminLedgerSource::MatchPang,
                        "match_experience" => AdminLedgerSource::MatchExperience,
                        "shop_pang" => AdminLedgerSource::ShopPang,
                        "item" => AdminLedgerSource::Item,
                        _ => return Err(RepositoryError::CorruptData),
                    },
                    delta: row.delta,
                    balance_after: row.balance_after,
                    reason: row.reason,
                    reference: row.reference,
                    created_at: row.created_at.into(),
                })
            })
            .collect()
    }

    async fn list_account_matches_inner(
        &self,
        account_id: AccountId,
        page: AdminPage,
    ) -> Result<Vec<AdminMatchEntry>, RepositoryError> {
        let rows = sqlx::query!(
            "SELECT m.id, m.mode, m.course_id, m.status, m.created_at, \
                    mp.strokes, mp.score, mp.place, mp.completion, \
                    mp.pang_reward, mp.experience_reward \
             FROM match_players mp JOIN matches m ON m.id = mp.match_id \
             WHERE mp.account_id = $1 \
             ORDER BY m.created_at DESC LIMIT $2 OFFSET $3",
            account_id.get(),
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;

        rows.into_iter()
            .map(|row| {
                Ok(AdminMatchEntry {
                    match_id: MatchId::new(row.id),
                    mode: row.mode,
                    course_id: CourseId::try_from(row.course_id)
                        .map_err(|_| RepositoryError::CorruptData)?,
                    status: row.status,
                    strokes: row.strokes,
                    score: row.score,
                    place: row.place,
                    completion: row.completion,
                    pang_reward: row.pang_reward,
                    experience_reward: row.experience_reward,
                    created_at: row.created_at.into(),
                })
            })
            .collect()
    }

    async fn set_balances_inner(
        &self,
        account_id: AccountId,
        assignment: BalanceAssignment,
    ) -> Result<AccountBalances, RepositoryError> {
        if assignment.is_empty() {
            return Err(RepositoryError::CorruptData);
        }
        let pang = assignment
            .pang
            .map(i64::try_from)
            .transpose()
            .map_err(|_| RepositoryError::BalanceOverflow)?;
        let points = assignment
            .points
            .map(i64::try_from)
            .transpose()
            .map_err(|_| RepositoryError::BalanceOverflow)?;
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        // The same row lock the credit path takes. Without it, an operator correcting a
        // balance while a match commits could clobber the reward.
        sqlx::query_scalar!(
            "SELECT account_id FROM profiles WHERE account_id = $1 FOR UPDATE",
            account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(repository_db_error)?
        .ok_or(RepositoryError::NotFound)?;
        let row = sqlx::query!(
            "UPDATE profiles \
             SET pang = COALESCE($2, pang), points = COALESCE($3, points), updated_at = now() \
             WHERE account_id = $1 RETURNING pang, points",
            account_id.get(),
            pang,
            points
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(AccountBalances {
            pang: checked_u64(row.pang)?,
            points: checked_u64(row.points)?,
        })
    }

    async fn set_credential_inner(
        &self,
        account_id: AccountId,
        hash: CredentialHash,
    ) -> Result<(), RepositoryError> {
        let updated = sqlx::query_scalar!(
            "UPDATE credentials SET password_hash = $2, updated_at = now() \
             WHERE account_id = $1 RETURNING account_id",
            account_id.get(),
            hash.expose_phc()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(repository_db_error)?;
        updated.map(|_| ()).ok_or(RepositoryError::NotFound)
    }
}

impl PgRepository {
    async fn grant_item_inner(
        &self,
        request: AdminItemGrant,
    ) -> Result<InventoryItem, AdminMutationError> {
        // Mirrors `ck_inventory_m7_shape`, so the caller gets a typed refusal instead of a
        // constraint violation collapsed into an opaque storage fault.
        match request.class {
            InventoryClass::Consumable if request.durability.is_some() => {
                return Err(AdminMutationError::InvalidShape);
            }
            InventoryClass::ClubSet | InventoryClass::Ball | InventoryClass::CharacterPart
                if request.quantity != 1 =>
            {
                return Err(AdminMutationError::InvalidShape);
            }
            _ if request.quantity == 0 => return Err(AdminMutationError::InvalidShape),
            _ => {}
        }
        let quantity = i64::from(request.quantity);
        let durability = request.durability.map(i64::from);
        let class = inventory_class_text_admin(request.class);
        // A distinct acquisition prefix, so an operator grant is distinguishable from a
        // starter grant and from a purchase forever after.
        let acquisition = format!("admin.{}", Uuid::new_v4().simple());
        let mut transaction = self.pool.begin().await.map_err(admin_db_error)?;
        sqlx::query_scalar!(
            "SELECT id FROM accounts WHERE id = $1 FOR UPDATE",
            request.account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(admin_db_error)?
        .ok_or(AdminMutationError::NotFound)?;

        if request.class == InventoryClass::Consumable {
            // Consumables carry a partial unique index on (account, type). Adding to the
            // existing row is what an operator means by "give them ten more".
            let existing = sqlx::query!(
                "SELECT id, quantity FROM inventory_items \
                 WHERE account_id = $1 AND item_type_id = $2 AND inventory_class = 'consumable' \
                 FOR UPDATE",
                request.account_id.get(),
                i64::from(request.item_type_id.get())
            )
            .fetch_optional(&mut *transaction)
            .await
            .map_err(admin_db_error)?;
            if let Some(row) = existing {
                let total = row
                    .quantity
                    .checked_add(quantity)
                    .ok_or(AdminMutationError::InvalidShape)?;
                let updated = sqlx::query_as!(
                    InventoryRow,
                    "UPDATE inventory_items SET quantity = $2, updated_at = now() \
                     WHERE id = $1 RETURNING id, account_id, item_type_id, quantity, \
                     starter_key, inventory_class, durability, expires_at",
                    row.id,
                    total
                )
                .fetch_one(&mut *transaction)
                .await
                .map_err(admin_db_error)?;
                transaction.commit().await.map_err(admin_db_error)?;
                return inventory_row_into_domain(updated).map_err(admin_domain_error);
            }
        }

        let inserted = sqlx::query_as!(
            InventoryRow,
            "INSERT INTO inventory_items \
             (account_id, item_type_id, starter_key, quantity, durability, inventory_class) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             RETURNING id, account_id, item_type_id, quantity, starter_key, inventory_class, \
                       durability, expires_at",
            request.account_id.get(),
            i64::from(request.item_type_id.get()),
            acquisition,
            quantity,
            durability,
            class
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(admin_db_error)?;
        transaction.commit().await.map_err(admin_db_error)?;
        inventory_row_into_domain(inserted).map_err(admin_domain_error)
    }

    async fn update_item_inner(
        &self,
        request: AdminItemUpdate,
    ) -> Result<InventoryItem, AdminMutationError> {
        if request.quantity == Some(0) {
            // `ck_inventory_quantity_positive`. Removing a row is `delete_item`.
            return Err(AdminMutationError::InvalidShape);
        }
        let mut transaction = self.pool.begin().await.map_err(admin_db_error)?;
        let row = sqlx::query!(
            "SELECT account_id, inventory_class FROM inventory_items WHERE id = $1 FOR UPDATE",
            request.inventory_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(admin_db_error)?
        .ok_or(AdminMutationError::NotFound)?;
        if row.account_id != request.account_id.get() {
            // Ownership is checked rather than assumed from the URL, so a mistyped id cannot
            // edit a different player's property.
            return Err(AdminMutationError::NotOwned);
        }
        let class = parse_inventory_class(&row.inventory_class).map_err(admin_domain_error)?;
        if class == InventoryClass::Consumable && matches!(request.durability, Some(Some(_))) {
            return Err(AdminMutationError::InvalidShape);
        }
        if matches!(
            class,
            InventoryClass::ClubSet | InventoryClass::Ball | InventoryClass::CharacterPart
        ) && request.quantity.is_some_and(|value| value != 1)
        {
            return Err(AdminMutationError::InvalidShape);
        }
        let updated = sqlx::query_as!(
            InventoryRow,
            "UPDATE inventory_items SET \
               quantity = COALESCE($2, quantity), \
               durability = CASE WHEN $3 THEN $4 ELSE durability END, \
               updated_at = now() \
             WHERE id = $1 \
             RETURNING id, account_id, item_type_id, quantity, starter_key, inventory_class, \
                       durability, expires_at",
            request.inventory_id.get(),
            request.quantity.map(i64::from),
            request.durability.is_some(),
            request.durability.flatten().map(i64::from)
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(admin_db_error)?;
        transaction.commit().await.map_err(admin_db_error)?;
        inventory_row_into_domain(updated).map_err(admin_domain_error)
    }

    async fn delete_item_inner(
        &self,
        account_id: AccountId,
        inventory_id: InventoryItemId,
    ) -> Result<(), AdminMutationError> {
        let mut transaction = self.pool.begin().await.map_err(admin_db_error)?;
        let row = sqlx::query!(
            "SELECT account_id FROM inventory_items WHERE id = $1 FOR UPDATE",
            inventory_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(admin_db_error)?
        .ok_or(AdminMutationError::NotFound)?;
        if row.account_id != account_id.get() {
            return Err(AdminMutationError::NotOwned);
        }
        // The equipment FK would refuse this anyway; catching it here turns a constraint
        // violation into an actionable message.
        let equipped = sqlx::query_scalar!(
            "SELECT count(*) FROM equipment_sets \
             WHERE account_id = $1 AND (club_item_id = $2 OR ball_item_id = $2)",
            account_id.get(),
            inventory_id.get()
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(admin_db_error)?
        .unwrap_or(0);
        if equipped > 0 {
            return Err(AdminMutationError::Equipped);
        }
        sqlx::query!(
            "DELETE FROM inventory_items WHERE id = $1",
            inventory_id.get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(admin_db_error)?;
        transaction.commit().await.map_err(admin_db_error)?;
        Ok(())
    }

    async fn grant_character_inner(
        &self,
        account_id: AccountId,
        item_type_id: ItemTypeId,
    ) -> Result<Character, AdminMutationError> {
        let acquisition = format!("admin.{}", Uuid::new_v4().simple());
        let row = sqlx::query!(
            "INSERT INTO characters (account_id, item_type_id, starter_key) \
             SELECT $1, $2, $3 FROM accounts WHERE id = $1 \
             RETURNING id, account_id, item_type_id, starter_key",
            account_id.get(),
            i64::from(item_type_id.get()),
            acquisition
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(admin_db_error)?
        .ok_or(AdminMutationError::NotFound)?;
        Ok(Character {
            id: CharacterId::new(row.id).map_err(|_| AdminMutationError::CorruptData)?,
            account_id: AccountId::new(row.account_id)
                .map_err(|_| AdminMutationError::CorruptData)?,
            item_type_id: ItemTypeId::try_from(row.item_type_id)
                .map_err(|_| AdminMutationError::CorruptData)?,
            starter_key: StarterKey::parse(&row.starter_key)
                .map_err(|_| AdminMutationError::CorruptData)?,
        })
    }

    async fn set_equipment_inner(
        &self,
        request: AdminEquipmentUpdate,
    ) -> Result<EquipmentSet, AdminMutationError> {
        let mut transaction = self.pool.begin().await.map_err(admin_db_error)?;
        let current = sqlx::query!(
            "SELECT version FROM equipment_sets WHERE account_id = $1 FOR UPDATE",
            request.account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(admin_db_error)?
        .ok_or(AdminMutationError::NotFound)?;
        if u32::try_from(current.version).map_err(|_| AdminMutationError::CorruptData)?
            != request.expected_version
        {
            // The same refusal the in-game path gives. An operator editing a stale page must
            // re-read rather than silently overwrite what changed underneath.
            return Err(AdminMutationError::VersionConflict);
        }
        // The version bump is what keeps a player's next in-game equip from being rejected —
        // and what makes a concurrent operator write visible rather than silent.
        let row = sqlx::query!(
            "UPDATE equipment_sets SET \
               character_id = $2, club_item_id = $3, ball_item_id = $4, \
               version = version + 1, updated_at = now() \
             WHERE account_id = $1 \
             RETURNING id, account_id, character_id, club_item_id, ball_item_id, version",
            request.account_id.get(),
            request.character_id.get(),
            request.club_item_id.map(InventoryItemId::get),
            request.ball_item_id.map(InventoryItemId::get)
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(admin_db_error)?;
        transaction.commit().await.map_err(admin_db_error)?;
        Ok(EquipmentSet {
            id: EquipmentSetId::new(row.id).map_err(|_| AdminMutationError::CorruptData)?,
            account_id: AccountId::new(row.account_id)
                .map_err(|_| AdminMutationError::CorruptData)?,
            character_id: CharacterId::new(row.character_id)
                .map_err(|_| AdminMutationError::CorruptData)?,
            club_item_id: row
                .club_item_id
                .map(InventoryItemId::new)
                .transpose()
                .map_err(|_| AdminMutationError::CorruptData)?,
            ball_item_id: row
                .ball_item_id
                .map(InventoryItemId::new)
                .transpose()
                .map_err(|_| AdminMutationError::CorruptData)?,
            version: u32::try_from(row.version).map_err(|_| AdminMutationError::CorruptData)?,
        })
    }
}

const fn inventory_class_text_admin(value: InventoryClass) -> &'static str {
    match value {
        InventoryClass::Legacy => "legacy",
        InventoryClass::ClubSet => "club_set",
        InventoryClass::Ball => "ball",
        InventoryClass::Consumable => "consumable",
        InventoryClass::CharacterPart => "character_part",
    }
}

/// Maps a database failure, naming the two constraints an operator can actually trip.
fn admin_db_error(error: sqlx::Error) -> AdminMutationError {
    let constraint = error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::constraint);
    match constraint {
        Some("uq_inventory_consumable_owner_type") => AdminMutationError::AlreadyStacked,
        Some("ck_inventory_m7_shape" | "ck_inventory_quantity_positive") => {
            AdminMutationError::InvalidShape
        }
        Some(
            "fk_equipment_club_owner" | "fk_equipment_ball_owner" | "fk_equipment_character_owner",
        ) => AdminMutationError::NotOwned,
        _ => AdminMutationError::Storage(storage_fault(&error)),
    }
}

/// Collapses a domain decode failure into the admin vocabulary.
fn admin_domain_error(_: RepositoryError) -> AdminMutationError {
    AdminMutationError::CorruptData
}

impl PgRepository {
    async fn load_shop_overlay_inner(&self) -> Result<ShopOverlay, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        // Revision and rows read in one transaction, so a snapshot can never carry a
        // revision that does not describe the rows beside it.
        let revision = sqlx::query_scalar!("SELECT revision FROM shop_overlay_revision")
            .fetch_one(&mut *transaction)
            .await
            .map_err(repository_db_error)?;
        let rows = sqlx::query!(
            "SELECT item_type_id, enabled, pang FROM shop_offer_overrides ORDER BY item_type_id"
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)?;
        let entries = rows
            .into_iter()
            .map(|row| {
                Ok(ShopOverride {
                    item_type_id: ItemTypeId::try_from(row.item_type_id)
                        .map_err(|_| RepositoryError::CorruptData)?,
                    enabled: row.enabled,
                    pang: row.pang.map(checked_u64).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, RepositoryError>>()?;
        Ok(ShopOverlay::new(revision, entries))
    }

    async fn set_shop_override_inner(
        &self,
        actor: AccountId,
        entry: ShopOverride,
        note: Option<String>,
    ) -> Result<i64, RepositoryError> {
        if entry.enabled.is_none() && entry.pang.is_none() {
            // Inheriting both fields is what "no override" means; the caller wants a delete.
            return Err(RepositoryError::CorruptData);
        }
        let pang = entry
            .pang
            .map(i64::try_from)
            .transpose()
            .map_err(|_| RepositoryError::BalanceOverflow)?;
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        sqlx::query!(
            "INSERT INTO shop_offer_overrides \
             (item_type_id, enabled, pang, note, updated_by, updated_at) \
             VALUES ($1, $2, $3, $4, $5, now()) \
             ON CONFLICT (item_type_id) DO UPDATE SET \
               enabled = EXCLUDED.enabled, pang = EXCLUDED.pang, note = EXCLUDED.note, \
               updated_by = EXCLUDED.updated_by, updated_at = now()",
            i64::from(entry.item_type_id.get()),
            entry.enabled,
            pang,
            note,
            actor.get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let revision = sqlx::query_scalar!("SELECT revision FROM shop_overlay_revision")
            .fetch_one(&mut *transaction)
            .await
            .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(revision)
    }

    async fn clear_shop_override_inner(
        &self,
        item_type_id: ItemTypeId,
    ) -> Result<i64, RepositoryError> {
        let mut transaction = self.pool.begin().await.map_err(repository_db_error)?;
        sqlx::query!(
            "DELETE FROM shop_offer_overrides WHERE item_type_id = $1",
            i64::from(item_type_id.get())
        )
        .execute(&mut *transaction)
        .await
        .map_err(repository_db_error)?;
        let revision = sqlx::query_scalar!("SELECT revision FROM shop_overlay_revision")
            .fetch_one(&mut *transaction)
            .await
            .map_err(repository_db_error)?;
        transaction.commit().await.map_err(repository_db_error)?;
        Ok(revision)
    }
}

impl PgRepository {
    async fn list_leaderboard_inner(
        &self,
        course_id: Option<CourseId>,
        page: AdminPage,
    ) -> Result<Vec<AdminLeaderboardEntry>, RepositoryError> {
        // Ordered by the same key as `ix_course_records_course_best`, so this reads the index
        // rather than sorting the table.
        let rows = sqlx::query!(
            "SELECT r.account_id, a.username_display, r.course_id, r.mode, r.best_score, \
                    r.best_strokes, r.rounds_completed, r.first_achieved_at \
             FROM course_records r JOIN accounts a ON a.id = r.account_id \
             WHERE ($1::bigint IS NULL OR r.course_id = $1) \
             ORDER BY r.best_score ASC, r.best_strokes ASC, r.first_achieved_at ASC \
             LIMIT $2 OFFSET $3",
            course_id.map(|value| i64::from(value.get())),
            page.limit(),
            page.offset()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(repository_db_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(AdminLeaderboardEntry {
                    account_id: AccountId::new(row.account_id)
                        .map_err(|_| RepositoryError::CorruptData)?,
                    username: row.username_display,
                    course_id: CourseId::try_from(row.course_id)
                        .map_err(|_| RepositoryError::CorruptData)?,
                    mode: row.mode,
                    best_score: row.best_score,
                    best_strokes: row.best_strokes,
                    rounds_completed: row.rounds_completed,
                    first_achieved_at: row.first_achieved_at.into(),
                })
            })
            .collect()
    }
}

impl AdminRepository for PgRepository {
    fn load_admin_authentication<'a>(
        &'a self,
        username: &'a NormalizedUsername,
    ) -> RepositoryFuture<'a, Result<Option<AdminAuthenticationRecord>, RepositoryError>> {
        Box::pin(self.observed(self.load_admin_authentication_inner(username)))
    }

    fn issue_admin_session(
        &self,
        request: NewAdminSession,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.issue_admin_session_inner(request)))
    }

    fn resolve_admin_session(
        &self,
        request: ResolveAdminSession,
    ) -> RepositoryFuture<'_, Result<Option<AdminSession>, RepositoryError>> {
        Box::pin(self.observed(self.resolve_admin_session_inner(request)))
    }

    fn revoke_admin_session(
        &self,
        id: AdminSessionId,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.revoke_admin_session_inner(id, now)))
    }

    fn revoke_admin_sessions_for_account(
        &self,
        account_id: AccountId,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.revoke_admin_sessions_for_account_inner(account_id, now)))
    }

    fn set_account_role(
        &self,
        account_id: AccountId,
        role: AccountRole,
        now: SystemTime,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.set_account_role_inner(account_id, role, now)))
    }

    fn record_admin_audit(
        &self,
        event: NewAdminAuditEvent,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.record_admin_audit_inner(event)))
    }

    fn list_admin_audit(
        &self,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminAuditEvent>, RepositoryError>> {
        Box::pin(self.observed(self.list_admin_audit_inner(page)))
    }

    fn list_accounts(
        &self,
        query: AdminAccountQuery,
    ) -> RepositoryFuture<'_, Result<Vec<AdminAccountSummary>, RepositoryError>> {
        Box::pin(self.observed(self.list_accounts_inner(query)))
    }

    fn load_account_detail(
        &self,
        account_id: AccountId,
    ) -> RepositoryFuture<'_, Result<AdminAccountDetail, RepositoryError>> {
        Box::pin(self.observed(self.load_account_detail_inner(account_id)))
    }

    fn list_account_ledger(
        &self,
        account_id: AccountId,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminLedgerEntry>, RepositoryError>> {
        Box::pin(self.observed(self.list_account_ledger_inner(account_id, page)))
    }

    fn list_account_matches(
        &self,
        account_id: AccountId,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminMatchEntry>, RepositoryError>> {
        Box::pin(self.observed(self.list_account_matches_inner(account_id, page)))
    }

    fn set_balances(
        &self,
        account_id: AccountId,
        assignment: BalanceAssignment,
    ) -> RepositoryFuture<'_, Result<AccountBalances, RepositoryError>> {
        Box::pin(self.observed(self.set_balances_inner(account_id, assignment)))
    }

    fn grant_item(
        &self,
        request: AdminItemGrant,
    ) -> RepositoryFuture<'_, Result<InventoryItem, AdminMutationError>> {
        Box::pin(self.observed(self.grant_item_inner(request)))
    }

    fn update_item(
        &self,
        request: AdminItemUpdate,
    ) -> RepositoryFuture<'_, Result<InventoryItem, AdminMutationError>> {
        Box::pin(self.observed(self.update_item_inner(request)))
    }

    fn delete_item(
        &self,
        account_id: AccountId,
        inventory_id: InventoryItemId,
    ) -> RepositoryFuture<'_, Result<(), AdminMutationError>> {
        Box::pin(self.observed(self.delete_item_inner(account_id, inventory_id)))
    }

    fn grant_character(
        &self,
        account_id: AccountId,
        item_type_id: ItemTypeId,
    ) -> RepositoryFuture<'_, Result<Character, AdminMutationError>> {
        Box::pin(self.observed(self.grant_character_inner(account_id, item_type_id)))
    }

    fn set_equipment(
        &self,
        request: AdminEquipmentUpdate,
    ) -> RepositoryFuture<'_, Result<EquipmentSet, AdminMutationError>> {
        Box::pin(self.observed(self.set_equipment_inner(request)))
    }

    fn load_shop_overlay(&self) -> RepositoryFuture<'_, Result<ShopOverlay, RepositoryError>> {
        Box::pin(self.observed(self.load_shop_overlay_inner()))
    }

    fn set_shop_override(
        &self,
        actor: AccountId,
        entry: ShopOverride,
        note: Option<String>,
    ) -> RepositoryFuture<'_, Result<i64, RepositoryError>> {
        Box::pin(self.observed(self.set_shop_override_inner(actor, entry, note)))
    }

    fn clear_shop_override(
        &self,
        item_type_id: ItemTypeId,
    ) -> RepositoryFuture<'_, Result<i64, RepositoryError>> {
        Box::pin(self.observed(self.clear_shop_override_inner(item_type_id)))
    }

    fn list_leaderboard(
        &self,
        course_id: Option<CourseId>,
        page: AdminPage,
    ) -> RepositoryFuture<'_, Result<Vec<AdminLeaderboardEntry>, RepositoryError>> {
        Box::pin(self.observed(self.list_leaderboard_inner(course_id, page)))
    }

    fn set_credential(
        &self,
        account_id: AccountId,
        hash: CredentialHash,
    ) -> RepositoryFuture<'_, Result<(), RepositoryError>> {
        Box::pin(self.observed(self.set_credential_inner(account_id, hash)))
    }
}
