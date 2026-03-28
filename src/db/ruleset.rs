use anyhow::Result;
use rusqlite::Connection;
use sea_query::{Expr, ExprTrait, OnConflict, Order, Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use crate::models::{Ruleset, RulesetActionConfig, RulesetActionType, RulesetRule, Subscription};

use super::iden::{RulesetFiles, RulesetRules, RulesetSubscriptions, Rulesets};

pub fn insert_ruleset(conn: &Connection, ruleset: &Ruleset) -> Result<i64> {
    let (sql, values) = Query::insert()
        .into_table(Rulesets::Table)
        .columns([Rulesets::Name, Rulesets::Description])
        .values_panic([
            ruleset.name.as_str().into(),
            ruleset.description.clone().into(),
        ])
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(conn.last_insert_rowid())
}

pub fn get_ruleset_by_name(conn: &Connection, name: &str) -> Result<Option<Ruleset>> {
    let (sql, values) = Query::select()
        .columns([Rulesets::Id, Rulesets::Name, Rulesets::Description])
        .from(Rulesets::Table)
        .and_where(Expr::col(Rulesets::Name).eq(name))
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(&*values.as_params(), row_to_ruleset)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn list_rulesets(conn: &Connection) -> Result<Vec<Ruleset>> {
    let (sql, values) = Query::select()
        .columns([Rulesets::Id, Rulesets::Name, Rulesets::Description])
        .from(Rulesets::Table)
        .order_by(Rulesets::Name, Order::Asc)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_ruleset)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn remove_ruleset(conn: &Connection, name: &str) -> Result<u64> {
    let Some(ruleset) = get_ruleset_by_name(conn, name)? else {
        return Ok(0);
    };
    let ruleset_id = ruleset.id.unwrap();

    let (sql, values) = Query::delete()
        .from_table(RulesetFiles::Table)
        .and_where(Expr::col(RulesetFiles::RulesetId).eq(ruleset_id))
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;

    let (sql, values) = Query::delete()
        .from_table(RulesetSubscriptions::Table)
        .and_where(Expr::col(RulesetSubscriptions::RulesetId).eq(ruleset_id))
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;

    let (sql, values) = Query::delete()
        .from_table(RulesetRules::Table)
        .and_where(Expr::col(RulesetRules::RulesetId).eq(ruleset_id))
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;

    let (sql, values) = Query::delete()
        .from_table(Rulesets::Table)
        .and_where(Expr::col(Rulesets::Id).eq(ruleset_id))
        .build_rusqlite(SqliteQueryBuilder);
    let count = conn.execute(&sql, &*values.as_params())?;
    Ok(count as u64)
}

pub fn insert_rule(conn: &Connection, rule: &RulesetRule) -> Result<i64> {
    let condition_json = rule
        .condition
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    let config_json = serde_json::to_string(&rule.action_config)?;

    let (sql, values) = Query::insert()
        .into_table(RulesetRules::Table)
        .columns([
            RulesetRules::RulesetId,
            RulesetRules::Priority,
            RulesetRules::Condition,
            RulesetRules::ActionType,
            RulesetRules::ActionConfig,
        ])
        .values_panic([
            rule.ruleset_id.into(),
            rule.priority.into(),
            condition_json.into(),
            rule.action_type.to_string().into(),
            config_json.into(),
        ])
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(conn.last_insert_rowid())
}

pub fn list_rules_for_ruleset(conn: &Connection, ruleset_id: i64) -> Result<Vec<RulesetRule>> {
    let (sql, values) = Query::select()
        .columns([
            RulesetRules::Id,
            RulesetRules::RulesetId,
            RulesetRules::Priority,
            RulesetRules::Condition,
            RulesetRules::ActionType,
            RulesetRules::ActionConfig,
        ])
        .from(RulesetRules::Table)
        .and_where(Expr::col(RulesetRules::RulesetId).eq(ruleset_id))
        .order_by(RulesetRules::Priority, Order::Asc)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_rule)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn subscribe_ruleset(conn: &Connection, ruleset_id: i64, reference: &str) -> Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    let (sql, values) = Query::insert()
        .into_table(RulesetSubscriptions::Table)
        .columns([
            RulesetSubscriptions::RulesetId,
            RulesetSubscriptions::Reference,
            RulesetSubscriptions::CreatedAt,
        ])
        .values_panic([ruleset_id.into(), reference.into(), now.into()])
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(conn.last_insert_rowid())
}

pub fn unsubscribe_ruleset(conn: &Connection, ruleset_id: i64, reference: &str) -> Result<u64> {
    let (sql, values) = Query::select()
        .column(RulesetSubscriptions::Id)
        .from(RulesetSubscriptions::Table)
        .and_where(Expr::col(RulesetSubscriptions::RulesetId).eq(ruleset_id))
        .and_where(Expr::col(RulesetSubscriptions::Reference).eq(reference))
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let sub_id: Option<i64> = stmt
        .query_map(&*values.as_params(), |row| row.get(0))?
        .next()
        .transpose()?;

    let Some(sub_id) = sub_id else {
        return Ok(0);
    };

    let (sql, values) = Query::delete()
        .from_table(RulesetFiles::Table)
        .and_where(Expr::col(RulesetFiles::SubscriptionId).eq(sub_id))
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;

    let (sql, values) = Query::delete()
        .from_table(RulesetSubscriptions::Table)
        .and_where(Expr::col(RulesetSubscriptions::Id).eq(sub_id))
        .build_rusqlite(SqliteQueryBuilder);
    let count = conn.execute(&sql, &*values.as_params())?;
    Ok(count as u64)
}

pub fn materialize_ruleset_file(
    conn: &Connection,
    ruleset_id: i64,
    sha256: &str,
    subscription_id: i64,
) -> Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let (sql, values) = Query::insert()
        .into_table(RulesetFiles::Table)
        .columns([
            RulesetFiles::RulesetId,
            RulesetFiles::Sha256,
            RulesetFiles::SubscriptionId,
            RulesetFiles::AttachedAt,
        ])
        .values_panic([
            ruleset_id.into(),
            sha256.into(),
            subscription_id.into(),
            now.into(),
        ])
        .on_conflict(
            OnConflict::columns([RulesetFiles::RulesetId, RulesetFiles::Sha256])
                .do_nothing()
                .to_owned(),
        )
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(())
}

pub fn get_rulesets_for_sha256(conn: &Connection, sha256: &str) -> Result<Vec<Ruleset>> {
    let (sql, values) = Query::select()
        .columns([
            (Rulesets::Table, Rulesets::Id),
            (Rulesets::Table, Rulesets::Name),
            (Rulesets::Table, Rulesets::Description),
        ])
        .from(Rulesets::Table)
        .inner_join(
            RulesetFiles::Table,
            Expr::col((RulesetFiles::Table, RulesetFiles::RulesetId))
                .equals((Rulesets::Table, Rulesets::Id)),
        )
        .and_where(Expr::col((RulesetFiles::Table, RulesetFiles::Sha256)).eq(sha256))
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_ruleset)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_all_ruleset_subscriptions(conn: &Connection) -> Result<Vec<(i64, Subscription)>> {
    let (sql, values) = Query::select()
        .columns([
            RulesetSubscriptions::RulesetId,
            RulesetSubscriptions::Id,
            RulesetSubscriptions::Reference,
            RulesetSubscriptions::CreatedAt,
        ])
        .from(RulesetSubscriptions::Table)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            Subscription {
                id: Some(row.get(1)?),
                reference: row.get(2)?,
                created_at: row.get(3)?,
            },
        ))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn row_to_ruleset(row: &rusqlite::Row) -> rusqlite::Result<Ruleset> {
    Ok(Ruleset {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        description: row.get(2)?,
    })
}

fn row_to_rule(row: &rusqlite::Row) -> rusqlite::Result<RulesetRule> {
    let condition_json: Option<String> = row.get(3)?;
    let action_type_str: String = row.get(4)?;
    let config_json: String = row.get(5)?;

    let condition = condition_json
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?;

    let action_type: RulesetActionType = action_type_str.parse().map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{e}"),
            )),
        )
    })?;

    let action_config: RulesetActionConfig = serde_json::from_str(&config_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;

    Ok(RulesetRule {
        id: Some(row.get(0)?),
        ruleset_id: row.get(1)?,
        priority: row.get(2)?,
        condition,
        action_type,
        action_config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ProjectDb;
    use crate::models::RuleCondition;
    use tempfile::TempDir;

    fn setup() -> (TempDir, ProjectDb) {
        let dir = TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        (dir, db)
    }

    #[test]
    fn ruleset_crud() {
        let (_dir, db) = setup();
        let rs = Ruleset {
            id: None,
            name: "evidence-policy".to_string(),
            description: Some("Policies for evidence files".to_string()),
        };
        let id = insert_ruleset(&db.conn(), &rs).unwrap();
        assert!(id > 0);

        let found = get_ruleset_by_name(&db.conn(), "evidence-policy")
            .unwrap()
            .unwrap();
        assert_eq!(found.name, "evidence-policy");

        let all = list_rulesets(&db.conn()).unwrap();
        assert_eq!(all.len(), 1);

        let removed = remove_ruleset(&db.conn(), "evidence-policy").unwrap();
        assert_eq!(removed, 1);
        assert!(list_rulesets(&db.conn()).unwrap().is_empty());
    }

    #[test]
    fn ruleset_rules() {
        let (_dir, db) = setup();
        let rs = Ruleset {
            id: None,
            name: "test-rules".to_string(),
            description: None,
        };
        let rs_id = insert_ruleset(&db.conn(), &rs).unwrap();

        let rule = RulesetRule {
            id: None,
            ruleset_id: rs_id,
            priority: 0,
            condition: None,
            action_type: RulesetActionType::ApplyPolicy,
            action_config: RulesetActionConfig {
                protection_level: Some("immutable".to_string()),
                command: None,
                env: None,
                quiet: None,
                file_type: None,
                tag: None,
                pipeline: None,
                sign_name: None,
            },
        };
        insert_rule(&db.conn(), &rule).unwrap();

        let rule2 = RulesetRule {
            id: None,
            ruleset_id: rs_id,
            priority: 1,
            condition: Some(RuleCondition {
                mime_type: Some("application/pdf".to_string()),
                file_type: None,
            }),
            action_type: RulesetActionType::DispatchTool,
            action_config: RulesetActionConfig {
                protection_level: None,
                command: Some("ocr".to_string()),
                env: None,
                quiet: Some(true),
                file_type: Some("*".to_string()),
                tag: None,
                pipeline: None,
                sign_name: None,
            },
        };
        insert_rule(&db.conn(), &rule2).unwrap();

        let rules = list_rules_for_ruleset(&db.conn(), rs_id).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].action_type, RulesetActionType::ApplyPolicy);
        assert_eq!(rules[1].action_type, RulesetActionType::DispatchTool);
        assert!(rules[1].condition.is_some());
    }

    #[test]
    fn ruleset_subscription_and_materialization() {
        let (_dir, db) = setup();
        let rs = Ruleset {
            id: None,
            name: "evidence-policy".to_string(),
            description: None,
        };
        let rs_id = insert_ruleset(&db.conn(), &rs).unwrap();
        let sub_id = subscribe_ruleset(&db.conn(), rs_id, ":evidence").unwrap();

        materialize_ruleset_file(&db.conn(), rs_id, "hash123", sub_id).unwrap();

        let found = get_rulesets_for_sha256(&db.conn(), "hash123").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "evidence-policy");

        let none = get_rulesets_for_sha256(&db.conn(), "other").unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn unsubscribe_cascades() {
        let (_dir, db) = setup();
        let rs = Ruleset {
            id: None,
            name: "test".to_string(),
            description: None,
        };
        let rs_id = insert_ruleset(&db.conn(), &rs).unwrap();
        let sub_id = subscribe_ruleset(&db.conn(), rs_id, ":evidence").unwrap();

        materialize_ruleset_file(&db.conn(), rs_id, "hash1", sub_id).unwrap();
        materialize_ruleset_file(&db.conn(), rs_id, "hash2", sub_id).unwrap();

        unsubscribe_ruleset(&db.conn(), rs_id, ":evidence").unwrap();

        assert!(get_rulesets_for_sha256(&db.conn(), "hash1")
            .unwrap()
            .is_empty());
    }
}
