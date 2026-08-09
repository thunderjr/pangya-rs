//! PostgreSQL implementation of the M7 local synthetic economy boundary.

use super::*;
use pangya_domain::{EconomyItemSelector, ItemDefinition};

#[derive(FromRow)]
struct OperationRow {
    operation_id: Uuid,
    account_id: i64,
    command: String,
    request_type_id: Option<i64>,
    request_quantity: Option<i64>,
    request_inventory_id: Option<i64>,
    request_expected_version: Option<i64>,
    request_character_id: Option<i64>,
    request_character_type_id: Option<i64>,
    request_club_item_id: Option<i64>,
    request_club_type_id: Option<i64>,
    request_ball_item_id: Option<i64>,
    request_ball_type_id: Option<i64>,
    result_inventory_id: Option<i64>,
    result_type_id: Option<i64>,
    result_quantity: Option<i64>,
    result_durability: Option<i64>,
    result_pang_balance: Option<i64>,
    result_pang_cost: Option<i64>,
    result_character_id: Option<i64>,
    result_club_item_id: Option<i64>,
    result_ball_item_id: Option<i64>,
    result_equipment_version: Option<i64>,
}

impl PgRepository {
    async fn purchase_economy(
        &self,
        request: PurchaseRequest,
    ) -> Result<EconomyCommit<PurchaseResult>, EconomyError> {
        validate_operation_id(request.operation_id)?;
        let mut transaction = self.pool.begin().await.map_err(economy_db_error)?;
        lock_operation(&mut transaction, request.operation_id).await?;
        if let Some(row) = load_operation(&mut transaction, request.operation_id).await? {
            let result = replay_purchase(&row, &request)?;
            transaction.commit().await.map_err(economy_db_error)?;
            return Ok(EconomyCommit::Replayed(result));
        }
        let price = match request.definition.sale {
            ItemSale::Pang(price) if price > 0 => price,
            ItemSale::NotSold | ItemSale::Pang(_) => return Err(EconomyError::Invalid),
        };
        if request.quantity == 0 || matches!(request.definition.kind, ItemKind::Character) {
            return Err(EconomyError::Invalid);
        }
        match request.definition.stacking {
            ItemStacking::Unique if request.quantity != 1 => return Err(EconomyError::Invalid),
            ItemStacking::Unique => {}
            ItemStacking::Stackable { max_stack } => {
                if max_stack == 0 || request.quantity > max_stack {
                    return Err(EconomyError::StackFull);
                }
            }
        }
        validate_definition_shape(request.definition)?;
        let total = price
            .checked_mul(u64::from(request.quantity))
            .ok_or(EconomyError::ArithmeticOverflow)?;
        let total_i64 = i64::try_from(total).map_err(|_| EconomyError::ArithmeticOverflow)?;
        lock_economy_account(&mut transaction, request.account_id).await?;
        let pang = lock_profile_pang(&mut transaction, request.account_id).await?;
        if pang < total_i64 {
            return Err(EconomyError::InsufficientPang);
        }
        let balance_after = pang
            .checked_sub(total_i64)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        let class = inventory_class_text(request.definition.kind)?;
        let (inventory_id, quantity_before, quantity_after, durability_after) = match request
            .definition
            .stacking
        {
            ItemStacking::Unique => {
                let durability = match request.definition.durability {
                    ItemDurability::Nondurable => None,
                    ItemDurability::Durable { max, .. } if max > 0 => Some(i64::from(max)),
                    ItemDurability::Durable { .. } => return Err(EconomyError::Invalid),
                };
                let acquisition = format!("purchase.{}", request.operation_id.get().simple());
                let id = sqlx::query_scalar!(
                        "INSERT INTO inventory_items \
                         (account_id, item_type_id, starter_key, quantity, durability, inventory_class) \
                         VALUES ($1, $2, $3, 1, $4, $5) RETURNING id",
                        request.account_id.get(),
                        i64::from(request.definition.type_id.get()),
                        acquisition,
                        durability,
                        class
                    )
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(economy_db_error)?;
                (id, 0_i64, 1_i64, durability)
            }
            ItemStacking::Stackable { max_stack } => {
                if !matches!(request.definition.kind, ItemKind::Consumable)
                    || !matches!(request.definition.durability, ItemDurability::Nondurable)
                {
                    return Err(EconomyError::Invalid);
                }
                let existing = sqlx::query!(
                    "SELECT id, quantity, durability FROM inventory_items \
                         WHERE account_id = $1 AND item_type_id = $2 \
                           AND inventory_class = 'consumable' ORDER BY id FOR UPDATE",
                    request.account_id.get(),
                    i64::from(request.definition.type_id.get())
                )
                .fetch_optional(&mut *transaction)
                .await
                .map_err(economy_db_error)?;
                if let Some(row) = existing {
                    if row.durability.is_some() || row.quantity <= 0 {
                        return Err(EconomyError::CorruptData);
                    }
                    let after = row
                        .quantity
                        .checked_add(i64::from(request.quantity))
                        .ok_or(EconomyError::ArithmeticOverflow)?;
                    if after > i64::from(max_stack) {
                        return Err(EconomyError::StackFull);
                    }
                    sqlx::query!(
                            "UPDATE inventory_items SET quantity = $2, updated_at = now() WHERE id = $1",
                            row.id,
                            after
                        )
                        .execute(&mut *transaction)
                        .await
                        .map_err(economy_db_error)?;
                    (row.id, row.quantity, after, None)
                } else {
                    let acquisition = format!("purchase.{}", request.operation_id.get().simple());
                    let id = sqlx::query_scalar!(
                        "INSERT INTO inventory_items \
                             (account_id, item_type_id, starter_key, quantity, inventory_class) \
                             VALUES ($1, $2, $3, $4, 'consumable') RETURNING id",
                        request.account_id.get(),
                        i64::from(request.definition.type_id.get()),
                        acquisition,
                        i64::from(request.quantity)
                    )
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(economy_db_error)?;
                    (id, 0_i64, i64::from(request.quantity), None)
                }
            }
        };
        sqlx::query!(
            "UPDATE profiles SET pang = $2, updated_at = now() WHERE account_id = $1",
            request.account_id.get(),
            balance_after
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        insert_purchase_operation(
            &mut transaction,
            &request,
            inventory_id,
            quantity_after,
            durability_after,
            balance_after,
            total_i64,
        )
        .await?;
        sqlx::query!(
            "INSERT INTO shop_currency_ledger \
             (operation_id, account_id, delta, reason, balance_after) \
             VALUES ($1, $2, $3, 'purchase', $4)",
            request.operation_id.get(),
            request.account_id.get(),
            -total_i64,
            balance_after
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let durability_delta = durability_after;
        sqlx::query!(
            "INSERT INTO item_ledger \
             (operation_id, account_id, inventory_id, item_type_id, quantity_delta, \
              quantity_after, durability_delta, durability_after, reason) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'purchase')",
            request.operation_id.get(),
            request.account_id.get(),
            inventory_id,
            i64::from(request.definition.type_id.get()),
            quantity_after - quantity_before,
            quantity_after,
            durability_delta,
            durability_after
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let result = PurchaseResult {
            operation_id: request.operation_id,
            inventory_id: InventoryItemId::new(inventory_id)
                .map_err(|_| EconomyError::CorruptData)?,
            item_type_id: request.definition.type_id,
            quantity_after: u32::try_from(quantity_after).map_err(|_| EconomyError::CorruptData)?,
            durability: durability_after
                .map(u32::try_from)
                .transpose()
                .map_err(|_| EconomyError::CorruptData)?,
            pang_balance: u64::try_from(balance_after).map_err(|_| EconomyError::CorruptData)?,
        };
        transaction.commit().await.map_err(economy_db_error)?;
        Ok(EconomyCommit::Committed(result))
    }

    async fn equip_economy(
        &self,
        request: EquipmentChange,
    ) -> Result<EconomyCommit<EquipmentChangeResult>, EconomyError> {
        validate_operation_id(request.operation_id)?;
        if let Some(club) = request.club {
            validate_selector(club, ItemKind::ClubSet)?;
        }
        if let Some(ball) = request.ball {
            validate_selector(ball, ItemKind::Ball)?;
        }
        if request.club.map(|item| item.inventory_id) == request.ball.map(|item| item.inventory_id)
            && request.club.is_some()
        {
            return Err(EconomyError::Incompatible);
        }
        let mut transaction = self.pool.begin().await.map_err(economy_db_error)?;
        lock_operation(&mut transaction, request.operation_id).await?;
        if let Some(row) = load_operation(&mut transaction, request.operation_id).await? {
            let result = replay_equip(&row, &request)?;
            transaction.commit().await.map_err(economy_db_error)?;
            return Ok(EconomyCommit::Replayed(result));
        }
        lock_economy_account(&mut transaction, request.account_id).await?;
        let _ = lock_profile_pang(&mut transaction, request.account_id).await?;
        let equipment = sqlx::query!(
            "SELECT version FROM equipment_sets WHERE account_id = $1 FOR UPDATE",
            request.account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(economy_db_error)?
        .ok_or(EconomyError::CorruptData)?;
        if equipment.version != i64::from(request.expected_version) {
            return Err(EconomyError::VersionConflict);
        }
        let character_type = sqlx::query_scalar!(
            "SELECT item_type_id FROM characters WHERE account_id = $1 AND id = $2 FOR UPDATE",
            request.account_id.get(),
            request.character_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(economy_db_error)?
        .ok_or(EconomyError::NotOwned)?;
        if character_type != i64::from(request.character_type_id.get()) {
            return Err(EconomyError::Incompatible);
        }
        let mut ids = [request.club, request.ball]
            .into_iter()
            .flatten()
            .map(|selector| selector.inventory_id.get())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        let rows = sqlx::query!(
            "SELECT id, item_type_id, quantity, durability, expires_at, inventory_class \
             FROM inventory_items WHERE account_id = $1 AND id = ANY($2) ORDER BY id FOR UPDATE",
            request.account_id.get(),
            &ids
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        if rows.len() != ids.len() {
            return Err(EconomyError::NotOwned);
        }
        for selector in [request.club, request.ball].into_iter().flatten() {
            let row = rows
                .iter()
                .find(|row| row.id == selector.inventory_id.get())
                .ok_or(EconomyError::NotOwned)?;
            validate_owned_item(
                row.item_type_id,
                row.quantity,
                row.durability,
                row.expires_at,
                &row.inventory_class,
                selector,
            )?;
            if selector.definition.kind == ItemKind::ClubSet
                && matches!(
                    selector.definition.durability,
                    ItemDurability::Durable { .. }
                )
                && row.durability == Some(0)
            {
                return Err(EconomyError::Depleted);
            }
        }
        let version = equipment
            .version
            .checked_add(1)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        sqlx::query!(
            "UPDATE profiles SET selected_character_id = $2, updated_at = now() \
             WHERE account_id = $1",
            request.account_id.get(),
            request.character_id.get()
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let club_id = request.club.map(|item| item.inventory_id.get());
        let ball_id = request.ball.map(|item| item.inventory_id.get());
        sqlx::query!(
            "UPDATE equipment_sets SET character_id = $2, club_item_id = $3, ball_item_id = $4, \
                    version = $5, updated_at = now() WHERE account_id = $1",
            request.account_id.get(),
            request.character_id.get(),
            club_id,
            ball_id,
            version
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        insert_equip_operation(&mut transaction, &request, version).await?;
        sqlx::query!(
            "INSERT INTO equipment_ledger \
             (operation_id, account_id, character_id, club_item_id, ball_item_id, version_after) \
             VALUES ($1, $2, $3, $4, $5, $6)",
            request.operation_id.get(),
            request.account_id.get(),
            request.character_id.get(),
            club_id,
            ball_id,
            version
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let result = EquipmentChangeResult {
            operation_id: request.operation_id,
            character_id: request.character_id,
            club_item_id: request.club.map(|item| item.inventory_id),
            ball_item_id: request.ball.map(|item| item.inventory_id),
            version: u32::try_from(version).map_err(|_| EconomyError::ArithmeticOverflow)?,
        };
        transaction.commit().await.map_err(economy_db_error)?;
        Ok(EconomyCommit::Committed(result))
    }

    async fn consume_economy(
        &self,
        request: ConsumeItem,
    ) -> Result<EconomyCommit<ConsumeItemResult>, EconomyError> {
        validate_operation_id(request.operation_id)?;
        let mut transaction = self.pool.begin().await.map_err(economy_db_error)?;
        lock_operation(&mut transaction, request.operation_id).await?;
        if let Some(row) = load_operation(&mut transaction, request.operation_id).await? {
            let result = replay_consume(&row, &request)?;
            transaction.commit().await.map_err(economy_db_error)?;
            return Ok(EconomyCommit::Replayed(result));
        }
        validate_selector(request.item, ItemKind::Consumable)?;
        if !matches!(
            request.item.definition.stacking,
            ItemStacking::Stackable { .. }
        ) {
            return Err(EconomyError::Incompatible);
        }
        lock_economy_account(&mut transaction, request.account_id).await?;
        let _ = lock_profile_pang(&mut transaction, request.account_id).await?;
        let _equipment = sqlx::query_scalar!(
            "SELECT version FROM equipment_sets WHERE account_id = $1 FOR UPDATE",
            request.account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let row = sqlx::query!(
            "SELECT item_type_id, quantity, durability, expires_at, inventory_class \
             FROM inventory_items WHERE account_id = $1 AND id = $2 FOR UPDATE",
            request.account_id.get(),
            request.item.inventory_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(economy_db_error)?
        .ok_or(EconomyError::NotOwned)?;
        validate_owned_item(
            row.item_type_id,
            row.quantity,
            row.durability,
            row.expires_at,
            &row.inventory_class,
            request.item,
        )?;
        let after = row
            .quantity
            .checked_sub(1)
            .ok_or(EconomyError::CorruptData)?;
        if after == 0 {
            sqlx::query!(
                "DELETE FROM inventory_items WHERE account_id = $1 AND id = $2",
                request.account_id.get(),
                request.item.inventory_id.get()
            )
            .execute(&mut *transaction)
            .await
            .map_err(economy_db_error)?;
        } else {
            sqlx::query!(
                "UPDATE inventory_items SET quantity = $3, updated_at = now() \
                 WHERE account_id = $1 AND id = $2",
                request.account_id.get(),
                request.item.inventory_id.get(),
                after
            )
            .execute(&mut *transaction)
            .await
            .map_err(economy_db_error)?;
        }
        insert_consume_operation(&mut transaction, &request, after).await?;
        sqlx::query!(
            "INSERT INTO item_ledger \
             (operation_id, account_id, inventory_id, item_type_id, quantity_delta, \
              quantity_after, reason) VALUES ($1, $2, $3, $4, -1, $5, 'consume')",
            request.operation_id.get(),
            request.account_id.get(),
            request.item.inventory_id.get(),
            i64::from(request.item.definition.type_id.get()),
            after
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let result = ConsumeItemResult {
            operation_id: request.operation_id,
            inventory_id: request.item.inventory_id,
            item_type_id: request.item.definition.type_id,
            quantity_after: u32::try_from(after).map_err(|_| EconomyError::CorruptData)?,
        };
        transaction.commit().await.map_err(economy_db_error)?;
        Ok(EconomyCommit::Committed(result))
    }

    async fn repair_economy(
        &self,
        request: RepairItem,
    ) -> Result<EconomyCommit<RepairItemResult>, EconomyError> {
        validate_operation_id(request.operation_id)?;
        let mut transaction = self.pool.begin().await.map_err(economy_db_error)?;
        lock_operation(&mut transaction, request.operation_id).await?;
        if let Some(row) = load_operation(&mut transaction, request.operation_id).await? {
            let result = replay_repair(&row, &request)?;
            transaction.commit().await.map_err(economy_db_error)?;
            return Ok(EconomyCommit::Replayed(result));
        }
        validate_selector(request.item, ItemKind::ClubSet)?;
        let (max, rate) = match request.item.definition.durability {
            ItemDurability::Durable {
                max,
                repair_pang_per_point,
            } if max > 0 && repair_pang_per_point > 0 => (max, repair_pang_per_point),
            ItemDurability::Nondurable | ItemDurability::Durable { .. } => {
                return Err(EconomyError::Invalid);
            }
        };
        lock_economy_account(&mut transaction, request.account_id).await?;
        let pang = lock_profile_pang(&mut transaction, request.account_id).await?;
        let _equipment = sqlx::query_scalar!(
            "SELECT version FROM equipment_sets WHERE account_id = $1 FOR UPDATE",
            request.account_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let row = sqlx::query!(
            "SELECT item_type_id, quantity, durability, expires_at, inventory_class \
             FROM inventory_items WHERE account_id = $1 AND id = $2 FOR UPDATE",
            request.account_id.get(),
            request.item.inventory_id.get()
        )
        .fetch_optional(&mut *transaction)
        .await
        .map_err(economy_db_error)?
        .ok_or(EconomyError::NotOwned)?;
        validate_owned_item(
            row.item_type_id,
            row.quantity,
            row.durability,
            row.expires_at,
            &row.inventory_class,
            request.item,
        )?;
        let current = u32::try_from(row.durability.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?;
        if current > max {
            return Err(EconomyError::CorruptData);
        }
        let missing = max.checked_sub(current).ok_or(EconomyError::CorruptData)?;
        if missing == 0 {
            return Err(EconomyError::Invalid);
        }
        let cost = u64::from(missing)
            .checked_mul(u64::from(rate))
            .ok_or(EconomyError::ArithmeticOverflow)?;
        let cost_i64 = i64::try_from(cost).map_err(|_| EconomyError::ArithmeticOverflow)?;
        if pang < cost_i64 {
            return Err(EconomyError::InsufficientPang);
        }
        let balance_after = pang
            .checked_sub(cost_i64)
            .ok_or(EconomyError::ArithmeticOverflow)?;
        sqlx::query!(
            "UPDATE inventory_items SET durability = $3, updated_at = now() \
             WHERE account_id = $1 AND id = $2",
            request.account_id.get(),
            request.item.inventory_id.get(),
            i64::from(max)
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        sqlx::query!(
            "UPDATE profiles SET pang = $2, updated_at = now() WHERE account_id = $1",
            request.account_id.get(),
            balance_after
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        insert_repair_operation(&mut transaction, &request, max, balance_after, cost_i64).await?;
        sqlx::query!(
            "INSERT INTO shop_currency_ledger \
             (operation_id, account_id, delta, reason, balance_after) \
             VALUES ($1, $2, $3, 'repair', $4)",
            request.operation_id.get(),
            request.account_id.get(),
            -cost_i64,
            balance_after
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        sqlx::query!(
            "INSERT INTO item_ledger \
             (operation_id, account_id, inventory_id, item_type_id, quantity_delta, \
              quantity_after, durability_delta, durability_after, reason) \
             VALUES ($1, $2, $3, $4, 0, 1, $5, $6, 'repair')",
            request.operation_id.get(),
            request.account_id.get(),
            request.item.inventory_id.get(),
            i64::from(request.item.definition.type_id.get()),
            i64::from(missing),
            i64::from(max)
        )
        .execute(&mut *transaction)
        .await
        .map_err(economy_db_error)?;
        let result = RepairItemResult {
            operation_id: request.operation_id,
            inventory_id: request.item.inventory_id,
            durability: max,
            pang_cost: cost,
            pang_balance: u64::try_from(balance_after).map_err(|_| EconomyError::CorruptData)?,
        };
        transaction.commit().await.map_err(economy_db_error)?;
        Ok(EconomyCommit::Committed(result))
    }
}

impl EconomyRepository for PgRepository {
    fn purchase(
        &self,
        request: PurchaseRequest,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<PurchaseResult>, EconomyError>> {
        Box::pin(self.observed(self.purchase_economy(request)))
    }

    fn equip(
        &self,
        request: EquipmentChange,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<EquipmentChangeResult>, EconomyError>> {
        Box::pin(self.observed(self.equip_economy(request)))
    }

    fn consume_one(
        &self,
        request: ConsumeItem,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<ConsumeItemResult>, EconomyError>> {
        Box::pin(self.observed(self.consume_economy(request)))
    }

    fn repair(
        &self,
        request: RepairItem,
    ) -> RepositoryFuture<'_, Result<EconomyCommit<RepairItemResult>, EconomyError>> {
        Box::pin(self.observed(self.repair_economy(request)))
    }
}

async fn lock_operation(
    transaction: &mut Transaction<'_, Postgres>,
    operation_id: EconomyOperationId,
) -> Result<(), EconomyError> {
    let key = operation_id.get().to_string();
    sqlx::query!("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))", key)
        .execute(&mut **transaction)
        .await
        .map_err(economy_db_error)?;
    Ok(())
}

async fn load_operation(
    transaction: &mut Transaction<'_, Postgres>,
    operation_id: EconomyOperationId,
) -> Result<Option<OperationRow>, EconomyError> {
    sqlx::query_as!(
        OperationRow,
        "SELECT operation_id, account_id, command, request_type_id, request_quantity, \
                request_inventory_id, request_expected_version, request_character_id, \
                request_character_type_id, request_club_item_id, request_club_type_id, \
                request_ball_item_id, request_ball_type_id, result_inventory_id, result_type_id, \
                result_quantity, result_durability, result_pang_balance, result_pang_cost, \
                result_character_id, result_club_item_id, result_ball_item_id, \
                result_equipment_version \
         FROM economy_operations WHERE operation_id = $1",
        operation_id.get()
    )
    .fetch_optional(&mut **transaction)
    .await
    .map_err(economy_db_error)
}

async fn lock_economy_account(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
) -> Result<(), EconomyError> {
    super::lock_active_account(transaction, account_id)
        .await
        .map_err(|error| match error {
            RepositoryError::NotFound => EconomyError::NotOwned,
            RepositoryError::AccountInactive => EconomyError::AccountInactive,
            RepositoryError::CorruptData => EconomyError::CorruptData,
            RepositoryError::Storage(fault) => EconomyError::Storage(fault),
            _ => EconomyError::Storage(StorageFault::Other),
        })
}

async fn lock_profile_pang(
    transaction: &mut Transaction<'_, Postgres>,
    account_id: AccountId,
) -> Result<i64, EconomyError> {
    sqlx::query_scalar!(
        "SELECT pang FROM profiles WHERE account_id = $1 FOR UPDATE",
        account_id.get()
    )
    .fetch_optional(&mut **transaction)
    .await
    .map_err(economy_db_error)?
    .ok_or(EconomyError::CorruptData)
}

fn validate_operation_id(operation_id: EconomyOperationId) -> Result<(), EconomyError> {
    if operation_id.get().is_nil() {
        Err(EconomyError::Invalid)
    } else {
        Ok(())
    }
}

fn validate_definition_shape(definition: ItemDefinition) -> Result<(), EconomyError> {
    match (definition.kind, definition.stacking, definition.durability) {
        (ItemKind::ClubSet, ItemStacking::Unique, _)
        | (
            ItemKind::Ball | ItemKind::CharacterPart,
            ItemStacking::Unique,
            ItemDurability::Nondurable,
        )
        | (
            ItemKind::Consumable,
            ItemStacking::Stackable { max_stack: 1.. },
            ItemDurability::Nondurable,
        ) => Ok(()),
        _ => Err(EconomyError::Invalid),
    }
}

fn validate_selector(selector: EconomyItemSelector, kind: ItemKind) -> Result<(), EconomyError> {
    if selector.definition.kind != kind {
        return Err(EconomyError::Incompatible);
    }
    validate_definition_shape(selector.definition)
}

fn inventory_class_text(kind: ItemKind) -> Result<&'static str, EconomyError> {
    match kind {
        ItemKind::ClubSet => Ok("club_set"),
        ItemKind::Ball => Ok("ball"),
        ItemKind::Consumable => Ok("consumable"),
        ItemKind::CharacterPart => Ok("character_part"),
        ItemKind::Caddie => Ok("caddie"),
        ItemKind::CaddieItem => Ok("caddie_item"),
        ItemKind::Mascot => Ok("mascot"),
        ItemKind::Card => Ok("card"),
        ItemKind::Furniture => Ok("furniture"),
        ItemKind::Skin => Ok("skin"),
        ItemKind::HairStyle => Ok("hair_style"),
        ItemKind::SetItem => Ok("set_item"),
        // A character is not an inventory row — owned characters live in `characters`, with
        // their own hair colour and mastery. Selling one needs a destination this path does not
        // have, so it stays refused. See docs/SPEC_SHOP_COVERAGE.md.
        ItemKind::Character => Err(EconomyError::Invalid),
    }
}

fn validate_owned_item(
    item_type_id: i64,
    quantity: i64,
    durability: Option<i64>,
    expires_at: Option<DateTime<Utc>>,
    class: &str,
    selector: EconomyItemSelector,
) -> Result<(), EconomyError> {
    if item_type_id != i64::from(selector.definition.type_id.get()) {
        return Err(EconomyError::Incompatible);
    }
    let expected = inventory_class_text(selector.definition.kind)?;
    if class != "legacy" && class != expected {
        return Err(EconomyError::Incompatible);
    }
    if quantity <= 0 {
        return Err(EconomyError::CorruptData);
    }
    if expires_at.is_some_and(|expiry| expiry <= Utc::now()) {
        return Err(EconomyError::Expired);
    }
    match (selector.definition.durability, durability) {
        (ItemDurability::Nondurable, None) => Ok(()),
        (ItemDurability::Durable { max, .. }, Some(value))
            if value >= 0 && value <= i64::from(max) =>
        {
            Ok(())
        }
        _ => Err(EconomyError::CorruptData),
    }
}

fn economy_db_error(error: sqlx::Error) -> EconomyError {
    EconomyError::Storage(super::storage_fault(&error))
}

fn same_account_command(row: &OperationRow, account: AccountId, command: &str) -> bool {
    row.operation_id != Uuid::nil() && row.account_id == account.get() && row.command == command
}

fn replay_purchase(
    row: &OperationRow,
    request: &PurchaseRequest,
) -> Result<PurchaseResult, EconomyError> {
    if !same_account_command(row, request.account_id, "purchase")
        || row.request_type_id != Some(i64::from(request.definition.type_id.get()))
        || row.request_quantity != Some(i64::from(request.quantity))
    {
        return Err(EconomyError::IdempotencyDrift);
    }
    Ok(PurchaseResult {
        operation_id: request.operation_id,
        inventory_id: InventoryItemId::new(
            row.result_inventory_id.ok_or(EconomyError::CorruptData)?,
        )
        .map_err(|_| EconomyError::CorruptData)?,
        item_type_id: ItemTypeId::try_from(row.result_type_id.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
        quantity_after: u32::try_from(row.result_quantity.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
        durability: row
            .result_durability
            .map(u32::try_from)
            .transpose()
            .map_err(|_| EconomyError::CorruptData)?,
        pang_balance: u64::try_from(row.result_pang_balance.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
    })
}

fn replay_equip(
    row: &OperationRow,
    request: &EquipmentChange,
) -> Result<EquipmentChangeResult, EconomyError> {
    let club = request.club.map(|item| item.inventory_id.get());
    let ball = request.ball.map(|item| item.inventory_id.get());
    if !same_account_command(row, request.account_id, "equip")
        || row.request_expected_version != Some(i64::from(request.expected_version))
        || row.request_character_id != Some(request.character_id.get())
        || row.request_character_type_id != Some(i64::from(request.character_type_id.get()))
        || row.request_club_item_id != club
        || row.request_club_type_id
            != request
                .club
                .map(|item| i64::from(item.definition.type_id.get()))
        || row.request_ball_item_id != ball
        || row.request_ball_type_id
            != request
                .ball
                .map(|item| i64::from(item.definition.type_id.get()))
    {
        return Err(EconomyError::IdempotencyDrift);
    }
    Ok(EquipmentChangeResult {
        operation_id: request.operation_id,
        character_id: CharacterId::new(row.result_character_id.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
        club_item_id: row
            .result_club_item_id
            .map(InventoryItemId::new)
            .transpose()
            .map_err(|_| EconomyError::CorruptData)?,
        ball_item_id: row
            .result_ball_item_id
            .map(InventoryItemId::new)
            .transpose()
            .map_err(|_| EconomyError::CorruptData)?,
        version: u32::try_from(
            row.result_equipment_version
                .ok_or(EconomyError::CorruptData)?,
        )
        .map_err(|_| EconomyError::CorruptData)?,
    })
}

fn replay_consume(
    row: &OperationRow,
    request: &ConsumeItem,
) -> Result<ConsumeItemResult, EconomyError> {
    if !same_account_command(row, request.account_id, "consume")
        || row.request_inventory_id != Some(request.item.inventory_id.get())
        || row.request_type_id != Some(i64::from(request.item.definition.type_id.get()))
    {
        return Err(EconomyError::IdempotencyDrift);
    }
    Ok(ConsumeItemResult {
        operation_id: request.operation_id,
        inventory_id: InventoryItemId::new(
            row.result_inventory_id.ok_or(EconomyError::CorruptData)?,
        )
        .map_err(|_| EconomyError::CorruptData)?,
        item_type_id: ItemTypeId::try_from(row.result_type_id.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
        quantity_after: u32::try_from(row.result_quantity.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
    })
}

fn replay_repair(
    row: &OperationRow,
    request: &RepairItem,
) -> Result<RepairItemResult, EconomyError> {
    if !same_account_command(row, request.account_id, "repair")
        || row.request_inventory_id != Some(request.item.inventory_id.get())
        || row.request_type_id != Some(i64::from(request.item.definition.type_id.get()))
    {
        return Err(EconomyError::IdempotencyDrift);
    }
    Ok(RepairItemResult {
        operation_id: request.operation_id,
        inventory_id: InventoryItemId::new(
            row.result_inventory_id.ok_or(EconomyError::CorruptData)?,
        )
        .map_err(|_| EconomyError::CorruptData)?,
        durability: u32::try_from(row.result_durability.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
        pang_cost: u64::try_from(row.result_pang_cost.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
        pang_balance: u64::try_from(row.result_pang_balance.ok_or(EconomyError::CorruptData)?)
            .map_err(|_| EconomyError::CorruptData)?,
    })
}

async fn insert_purchase_operation(
    transaction: &mut Transaction<'_, Postgres>,
    request: &PurchaseRequest,
    inventory_id: i64,
    quantity_after: i64,
    durability: Option<i64>,
    balance_after: i64,
    cost: i64,
) -> Result<(), EconomyError> {
    sqlx::query!(
        "INSERT INTO economy_operations \
         (operation_id, account_id, command, catalog_sha256, request_type_id, request_quantity, \
          result_inventory_id, result_type_id, result_quantity, result_durability, \
          result_pang_balance, result_pang_cost) \
         VALUES ($1, $2, 'purchase', $3, $4, $5, $6, $4, $7, $8, $9, $10)",
        request.operation_id.get(),
        request.account_id.get(),
        request.catalog.as_bytes().as_slice(),
        i64::from(request.definition.type_id.get()),
        i64::from(request.quantity),
        inventory_id,
        quantity_after,
        durability,
        balance_after,
        cost
    )
    .execute(&mut **transaction)
    .await
    .map_err(economy_db_error)?;
    Ok(())
}

async fn insert_equip_operation(
    transaction: &mut Transaction<'_, Postgres>,
    request: &EquipmentChange,
    version: i64,
) -> Result<(), EconomyError> {
    let club_id = request.club.map(|item| item.inventory_id.get());
    let club_type = request
        .club
        .map(|item| i64::from(item.definition.type_id.get()));
    let ball_id = request.ball.map(|item| item.inventory_id.get());
    let ball_type = request
        .ball
        .map(|item| i64::from(item.definition.type_id.get()));
    sqlx::query!(
        "INSERT INTO economy_operations \
         (operation_id, account_id, command, catalog_sha256, request_expected_version, \
          request_character_id, request_character_type_id, request_club_item_id, \
          request_club_type_id, request_ball_item_id, request_ball_type_id, \
          result_character_id, result_club_item_id, result_ball_item_id, result_equipment_version) \
         VALUES ($1, $2, 'equip', $3, $4, $5, $6, $7, $8, $9, $10, $5, $7, $9, $11)",
        request.operation_id.get(),
        request.account_id.get(),
        request.catalog.as_bytes().as_slice(),
        i64::from(request.expected_version),
        request.character_id.get(),
        i64::from(request.character_type_id.get()),
        club_id,
        club_type,
        ball_id,
        ball_type,
        version
    )
    .execute(&mut **transaction)
    .await
    .map_err(economy_db_error)?;
    Ok(())
}

async fn insert_consume_operation(
    transaction: &mut Transaction<'_, Postgres>,
    request: &ConsumeItem,
    quantity_after: i64,
) -> Result<(), EconomyError> {
    sqlx::query!(
        "INSERT INTO economy_operations \
         (operation_id, account_id, command, catalog_sha256, request_type_id, request_inventory_id, \
          result_inventory_id, result_type_id, result_quantity) \
         VALUES ($1, $2, 'consume', $3, $4, $5, $5, $4, $6)",
        request.operation_id.get(), request.account_id.get(), request.catalog.as_bytes().as_slice(),
        i64::from(request.item.definition.type_id.get()), request.item.inventory_id.get(), quantity_after
    ).execute(&mut **transaction).await.map_err(economy_db_error)?;
    Ok(())
}

async fn insert_repair_operation(
    transaction: &mut Transaction<'_, Postgres>,
    request: &RepairItem,
    durability: u32,
    balance_after: i64,
    cost: i64,
) -> Result<(), EconomyError> {
    sqlx::query!(
        "INSERT INTO economy_operations \
         (operation_id, account_id, command, catalog_sha256, request_type_id, request_inventory_id, \
          result_inventory_id, result_type_id, result_quantity, result_durability, \
          result_pang_balance, result_pang_cost) \
         VALUES ($1, $2, 'repair', $3, $4, $5, $5, $4, 1, $6, $7, $8)",
        request.operation_id.get(), request.account_id.get(), request.catalog.as_bytes().as_slice(),
        i64::from(request.item.definition.type_id.get()), request.item.inventory_id.get(),
        i64::from(durability), balance_after, cost
    ).execute(&mut **transaction).await.map_err(economy_db_error)?;
    Ok(())
}
