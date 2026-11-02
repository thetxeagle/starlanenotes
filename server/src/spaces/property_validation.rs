//! Property wire values stay strings; validation never converts or discards stored data.
use crate::auth::routes::AppError;
use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde_json::Value;
use sqlx::AnyConnection;
use std::collections::HashSet;

pub const TYPES: &[&str] = &[
    "text",
    "long_text",
    "number",
    "date",
    "datetime",
    "checkbox",
    "select",
    "multi_select",
    "rating",
    "url",
    "user",
    "relation",
];

pub fn validate_schema(name: &str, kind: &str, options: Option<&Value>) -> Result<(), AppError> {
    if name.trim().is_empty() || name.len() > 500 {
        return Err(AppError::BadRequest(
            "Property name must contain 1 to 500 bytes".into(),
        ));
    }
    if !TYPES.contains(&kind) {
        return Err(AppError::BadRequest(format!(
            "Invalid property type: {kind}"
        )));
    }
    if matches!(kind, "select" | "multi_select") {
        permitted_options(options)?;
    }
    Ok(())
}

fn permitted_options(options: Option<&Value>) -> Result<Vec<&str>, AppError> {
    let Some(options) = options.filter(|v| !v.is_null()) else {
        return Ok(vec![]);
    };
    let array = options
        .as_array()
        .or_else(|| options.get("options").and_then(Value::as_array))
        .ok_or_else(|| {
            AppError::BadRequest(
                "Select options must be an array of unique nonempty strings".into(),
            )
        })?;
    let mut seen = HashSet::new();
    array
        .iter()
        .map(|value| {
            let option = value
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .ok_or_else(|| {
                    AppError::BadRequest("Select options must be nonempty strings".into())
                })?;
            if !seen.insert(option) {
                return Err(AppError::BadRequest("Select options must be unique".into()));
            }
            Ok(option)
        })
        .collect()
}

/// Null and the exact empty string mean unset for every type. Nonempty values are not normalized.
pub fn validate_value(
    kind: &str,
    options: Option<&Value>,
    value: Option<&str>,
) -> Result<(), AppError> {
    if !TYPES.contains(&kind) {
        return Err(AppError::BadRequest(format!(
            "Invalid property type: {kind}"
        )));
    }
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return Ok(());
    };
    if value.len() > 100_000 {
        return Err(AppError::BadRequest(
            "Property value exceeds 100,000 bytes".into(),
        ));
    }
    let valid = match kind {
        "text" | "long_text" | "user" | "relation" => true,
        "number" => value.trim() == value && value.parse::<f64>().is_ok_and(f64::is_finite),
        "date" => value.len() == 10 && NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok(),
        "datetime" => {
            DateTime::parse_from_rfc3339(value).is_ok()
                || [
                    "%Y-%m-%dT%H:%M",
                    "%Y-%m-%dT%H:%M:%S",
                    "%Y-%m-%dT%H:%M:%S%.f",
                ]
                .iter()
                .any(|format| NaiveDateTime::parse_from_str(value, format).is_ok())
        }
        "checkbox" => matches!(value, "true" | "false"),
        "rating" => matches!(value, "0" | "1" | "2" | "3" | "4" | "5"),
        "url" => reqwest::Url::parse(value)
            .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some()),
        "select" => permitted_options(options)?.contains(&value),
        "multi_select" => {
            let options = permitted_options(options)?;
            serde_json::from_str::<Vec<String>>(value).is_ok_and(|selected| {
                let mut seen = HashSet::new();
                selected
                    .iter()
                    .all(|item| options.contains(&item.as_str()) && seen.insert(item))
            })
        }
        _ => false,
    };
    if !valid {
        return Err(AppError::BadRequest(format!(
            "Property value does not match {kind}; clear it or supply a valid value"
        )));
    }
    Ok(())
}

/// Validate a batch while holding schemas and global user targets in stable order.
/// The caller supplies an active transaction and establishes effective source access.
/// This helper does not authorize the source page or acquire its space guard.
/// User rows are global: ordering by property ID alone cannot prevent cross-space
/// batches from locking the same two target accounts in opposite orders.
pub async fn validate_assignment_batch(
    conn: &mut AnyConnection,
    space_id: &str,
    principal: super::access::Principal<'_>,
    assignments: &[(&str, Option<&str>)],
) -> Result<Vec<String>, AppError> {
    let mut relations = std::collections::BTreeSet::new();
    let mut ordered = assignments.to_vec();
    ordered.sort_by_key(|entry| entry.0);
    let mut users = std::collections::BTreeSet::new();
    for (property_id, value) in &ordered {
        let locked =
            sqlx::query("UPDATE properties_schema SET name = name WHERE id = $1 AND space_id = $2")
                .bind(property_id)
                .bind(space_id)
                .execute(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        if locked.rows_affected() != 1 {
            return Err(AppError::BadRequest(
                "Property not found in this space".into(),
            ));
        }
        let kind: String = sqlx::query_scalar(
            "SELECT property_type FROM properties_schema WHERE id = $1 AND space_id = $2",
        )
        .bind(property_id)
        .bind(space_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(AppError::internal)?;
        if kind == "relation" {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                relations.insert(value.to_string());
            }
        }
        if kind == "user" {
            if let Some(value) = value.filter(|value| !value.is_empty()) {
                users.insert(value);
            }
        }
    }
    for target in users {
        let available =
            sqlx::query("UPDATE users SET is_active = is_active WHERE id = $1 AND is_active = 1")
                .bind(target)
                .execute(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        if available.rows_affected() != 1 {
            return Err(AppError::BadRequest("User target is unavailable".into()));
        }
    }
    // Relation rows use deterministic target-ID order independently of schema IDs.
    for target in &relations {
        let locked = sqlx::query("UPDATE documents SET title=title WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL")
            .bind(target).bind(space_id).execute(&mut *conn).await.map_err(AppError::internal)?;
        if locked.rows_affected() != 1 {
            return Err(relation_unavailable());
        }
    }
    // Schema rows cannot change under these locks; each user lock below is already
    // owned, so the existing single-assignment checks cannot invert target order.
    for (property_id, value) in ordered {
        validate_assignment(conn, space_id, principal, property_id, value).await?;
    }
    Ok(relations.into_iter().collect())
}

/// Single-assignment validation; production batches use validate_assignment_batch
/// to acquire all global target-user locks in stable order before these checks.
pub async fn validate_assignment(
    conn: &mut AnyConnection,
    space_id: &str,
    principal: super::access::Principal<'_>,
    property_id: &str,
    value: Option<&str>,
) -> Result<(), AppError> {
    // A scoped no-op UPDATE acquires the same row lock as schema edits on both
    // PostgreSQL and SQLite. Keep it until the caller commits its transaction.
    let locked =
        sqlx::query("UPDATE properties_schema SET name = name WHERE id = $1 AND space_id = $2")
            .bind(property_id)
            .bind(space_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if locked.rows_affected() == 0 {
        return Err(AppError::BadRequest(
            "Property binding is not in this space".into(),
        ));
    }
    let schema: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT property_type, options FROM properties_schema WHERE id = $1 AND space_id = $2",
    )
    .bind(property_id)
    .bind(space_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let (kind, raw_options) = schema
        .ok_or_else(|| AppError::BadRequest("Property binding is not in this space".into()))?;
    let options = raw_options
        .map(|raw| serde_json::from_str::<Value>(&raw))
        .transpose()
        .map_err(|_| {
            AppError::BadRequest("Property options are invalid; repair the schema first".into())
        })?;
    validate_value(&kind, options.as_ref(), value)?;
    let Some(value) = value.filter(|v| !v.is_empty()) else {
        return Ok(());
    };
    if kind == "relation" {
        let locked = sqlx::query("UPDATE documents SET title=title WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL")
            .bind(value).bind(space_id).execute(&mut *conn).await.map_err(AppError::internal)?;
        if locked.rows_affected() != 1 {
            return Err(relation_unavailable());
        }
        revalidate_relation_targets(conn, space_id, principal, &[value.to_owned()]).await?;
    }
    if kind == "user" {
        let available =
            sqlx::query("UPDATE users SET is_active = is_active WHERE id = $1 AND is_active = 1")
                .bind(value)
                .execute(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        if available.rows_affected() != 1 {
            return Err(AppError::BadRequest(
                "User property must reference an active account".into(),
            ));
        }
    }
    Ok(())
}

fn relation_unavailable() -> AppError {
    AppError::BadRequest("Relation target is unavailable in this space".into())
}

/// Recheck only the distinct relation IDs validated by this transaction, after
/// canonical writes and their possible waits. Does not authorize the source.
pub async fn revalidate_relation_targets(
    conn: &mut AnyConnection,
    space_id: &str,
    principal: super::access::Principal<'_>,
    targets: &[String],
) -> Result<(), AppError> {
    for target in targets {
        super::access::resolve_page_access(
            conn,
            principal,
            space_id,
            target,
            super::access::PageOperation::ReferenceFromMutation,
            super::access::Purpose::Routine,
        )
        .await
        .map_err(|error| match error {
            AppError::NotFound(_) | AppError::Conflict(_) | AppError::BadRequest(_) => {
                relation_unavailable()
            }
            other => other,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_wire_types_without_reinterpreting_values() {
        for kind in TYPES {
            assert!(validate_value(kind, None, None).is_ok());
            assert!(validate_value(kind, None, Some("")).is_ok());
        }
        for (kind, value) in [
            ("number", "12.5"),
            ("number", ".5"),
            ("number", "01"),
            ("number", "-0.5"),
            ("number", "1e3"),
            ("date", "2026-10-07"),
            ("datetime", "2026-10-07T13:30"),
            ("datetime", "2026-10-07T13:30:00Z"),
            ("checkbox", "false"),
            ("rating", "0"),
            ("url", "https://example.test/a"),
        ] {
            assert!(
                validate_value(kind, None, Some(value)).is_ok(),
                "{kind} {value}"
            );
        }
        for (kind, value) in [
            ("number", "NaN"),
            ("number", "inf"),
            ("number", "-Infinity"),
            ("number", " 1"),
            ("number", "1 "),
            ("number", "\t1"),
            ("number", "1e999"),
            ("date", "2026-02-30"),
            ("datetime", "tomorrow"),
            ("checkbox", "1"),
            ("rating", "6"),
            ("rating", "1.0"),
            ("url", "javascript:alert(1)"),
        ] {
            assert!(
                validate_value(kind, None, Some(value)).is_err(),
                "{kind} {value}"
            );
        }
    }
    #[test]
    fn select_options_and_values_are_strict() {
        for options in [
            serde_json::json!(["A", "B"]),
            serde_json::json!({"options":["A","B"]}),
        ] {
            assert!(validate_schema("Choice", "select", Some(&options)).is_ok());
            assert!(validate_value("select", Some(&options), Some("A")).is_ok());
            assert!(validate_value("select", Some(&options), Some("C")).is_err());
            assert!(validate_value("multi_select", Some(&options), Some("[\"A\",\"B\"]")).is_ok());
            for invalid in ["[\"A\",\"A\"]", "[\"C\"]", "[1]", "A"] {
                assert!(validate_value("multi_select", Some(&options), Some(invalid)).is_err());
            }
        }
        for options in [
            serde_json::json!(["A", "A"]),
            serde_json::json!([""]),
            serde_json::json!([1]),
            serde_json::json!({}),
        ] {
            assert!(validate_schema("Choice", "select", Some(&options)).is_err());
        }
    }
}
