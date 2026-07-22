use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use sqlx::AnyPool;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;
const WEBHOOK_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Dispatch a webhook event to all matching webhooks
pub async fn dispatch_event(
    db: &AnyPool,
    event_type: &str,
    payload: &serde_json::Value,
    space_id: Option<&str>,
) {
    // Find matching webhooks
    let webhooks: Vec<(String, String, Option<String>, String)> = if let Some(sid) = space_id {
        sqlx::query_as(
            "SELECT id, url, secret_hash, events FROM webhooks \
             WHERE is_active = 1 AND (space_id = ?1 OR space_id IS NULL)",
        )
        .bind(sid)
        .fetch_all(db)
        .await
        .unwrap_or_default()
    } else {
        sqlx::query_as(
            "SELECT id, url, secret_hash, events FROM webhooks \
             WHERE is_active = 1 AND space_id IS NULL",
        )
        .fetch_all(db)
        .await
        .unwrap_or_default()
    };

    let payload_str = serde_json::to_string(payload).unwrap_or_default();

    for (webhook_id, url, secret_hash, events_json) in webhooks {
        // Check if this webhook is subscribed to this event
        let events: Vec<String> = serde_json::from_str(&events_json).unwrap_or_default();
        if !webhook_event_matches(&events, event_type) {
            continue;
        }

        // Spawn delivery in background
        let db = db.clone();
        let webhook_id = webhook_id.clone();
        let url = url.clone();
        let secret_hash = secret_hash.clone();
        let payload_str = payload_str.clone();
        let event_type = event_type.to_string();

        tokio::spawn(async move {
            deliver_webhook(
                &db,
                &webhook_id,
                &url,
                secret_hash.as_deref(),
                &event_type,
                &payload_str,
                1,
            )
            .await;
        });
    }
}

async fn deliver_webhook(
    db: &AnyPool,
    webhook_id: &str,
    url: &str,
    secret_hash: Option<&str>,
    event_type: &str,
    payload: &str,
    attempt: i32,
) {
    let client = Client::builder()
        .timeout(WEBHOOK_REQUEST_TIMEOUT)
        .build()
        .expect("static webhook client configuration is valid");
    let delivery_id = Uuid::new_v4().to_string();

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("X-Starlane-Event", event_type)
        .header("X-Starlane-Delivery", &delivery_id);

    // Add HMAC signature if secret is configured
    if let Some(_hash) = secret_hash {
        // We don't have the original secret (only the hash), so we use the hash as HMAC key
        // In practice, the secret would be stored encrypted and decrypted here
        if let Ok(mut mac) = HmacSha256::new_from_slice(_hash.as_bytes()) {
            mac.update(payload.as_bytes());
            let signature = hex::encode(mac.finalize().into_bytes());
            request = request.header("X-Starlane-Signature", format!("sha256={}", signature));
        }
    }

    let result = request.body(payload.to_string()).send().await;

    let (status, success, response_body) = match result {
        Ok(resp) => {
            let status = resp.status().as_u16() as i32;
            let body = resp.text().await.unwrap_or_default();
            let success = (200..300).contains(&(status as u16).into());
            (Some(status), success, Some(body))
        }
        Err(e) => {
            tracing::warn!("Webhook delivery failed for {}: {}", webhook_id, e);
            (None, false, Some(e.to_string()))
        }
    };

    // Record delivery
    let _ = sqlx::query(
        "INSERT INTO webhook_deliveries (id, webhook_id, event_type, payload, response_status, response_body, attempt_number, success) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&delivery_id)
    .bind(webhook_id)
    .bind(event_type)
    .bind(payload)
    .bind(status)
    .bind(&response_body)
    .bind(attempt)
    .bind(success)
    .execute(db)
    .await;

    // Retry with exponential backoff (max 3 attempts)
    if !success && attempt < 3 {
        let delay = std::time::Duration::from_secs(2u64.pow(attempt as u32));
        tokio::time::sleep(delay).await;

        Box::pin(deliver_webhook(
            db,
            webhook_id,
            url,
            secret_hash,
            event_type,
            payload,
            attempt + 1,
        ))
        .await;
    }
}

fn webhook_event_matches(events: &[String], event_type: &str) -> bool {
    events
        .iter()
        .any(|event| event == event_type || event == "*")
}

pub async fn redeliver_recorded_delivery(
    db: &AnyPool,
    webhook_id: &str,
    delivery_id: &str,
) -> Result<bool, sqlx::Error> {
    let delivery: Option<(String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT w.url, w.secret_hash, d.event_type, d.payload \
         FROM webhook_deliveries d \
         INNER JOIN webhooks w ON w.id = d.webhook_id \
         WHERE d.webhook_id = ?1 AND d.id = ?2",
    )
    .bind(webhook_id)
    .bind(delivery_id)
    .fetch_optional(db)
    .await?;

    let Some((url, secret_hash, event_type, payload)) = delivery else {
        return Ok(false);
    };

    let db = db.clone();
    let webhook_id = webhook_id.to_string();
    tokio::spawn(async move {
        deliver_webhook(
            &db,
            &webhook_id,
            &url,
            secret_hash.as_deref(),
            &event_type,
            &payload,
            1,
        )
        .await;
    });

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::webhook_event_matches;

    #[test]
    fn webhook_event_matching_supports_exact_and_wildcard_subscriptions() {
        assert!(webhook_event_matches(
            &["document.updated".to_string()],
            "document.updated"
        ));
        assert!(webhook_event_matches(&["*".to_string()], "space.created"));
        assert!(!webhook_event_matches(
            &["document.updated".to_string()],
            "document.deleted"
        ));
        assert!(!webhook_event_matches(
            &[" document.updated ".to_string()],
            "document.updated"
        ));
    }
}
