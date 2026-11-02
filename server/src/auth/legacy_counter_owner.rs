async fn legacy_counter_original_login(
    State(state): State<AppState>,
    Json(req): Json<PasskeyLoginFinishRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (challenge_user_id, _, authentication_state): (
        Option<String>,
        Option<String>,
        PasskeyAuthentication,
    ) = take_passkey_challenge(&state, &req.challenge_id, "authentication",None).await?;
    let user_id = challenge_user_id
        .ok_or_else(|| AppError::BadRequest("Passkey challenge is missing account state".into()))?;
    let webauthn = webauthn_for_state(&state)?;
    let credential: PublicKeyCredential = serde_json::from_value(req.credential)
        .map_err(|_| AppError::BadRequest("Passkey sign-in response is malformed".into()))?;
    let result = webauthn
        .finish_passkey_authentication(&credential, &authentication_state)
        .map_err(|error| AppError::Unauthorized(format!("Passkey sign-in failed: {error}")))?;
    let credential_id = credential_id_to_string(result.cred_id())?;

    let passkey_row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, public_key FROM passkeys WHERE user_id = $1 AND credential_id = $2",
    )
    .bind(&user_id)
    .bind(&credential_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    let (passkey_id, passkey_json) = passkey_row.ok_or_else(|| {
        AppError::Unauthorized("Passkey is not registered for this account".into())
    })?;

    let mut passkey: Passkey = serde_json::from_str(&passkey_json).map_err(AppError::internal)?;
    let mut updated_passkey_json = None;
    if result.needs_update() {
        passkey.update_credential(&result);
        updated_passkey_json = Some(serde_json::to_string(&passkey).map_err(AppError::internal)?);
    }
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(updated) = updated_passkey_json {
        sqlx::query(
            "UPDATE passkeys SET public_key = $1, sign_count = $2, last_used_at = $3 WHERE id = $4",
        )
        .bind(updated)
        .bind(i64::from(result.counter()))
        .bind(&now)
        .bind(&passkey_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    } else {
        sqlx::query("UPDATE passkeys SET last_used_at = $1 WHERE id = $2")
            .bind(&now)
            .bind(&passkey_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    let user: (String, Option<String>, String, Option<String>, String, i32) = sqlx::query_as(
        "SELECT email, username, display_name, avatar_url, role, totp_enabled FROM users WHERE id = $1 AND is_active = 1",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::internal)?;
    let tokens =
        create_session_tokens(&user_id, &user.0, &user.4, &state.jwt_secret, &req.session)?;
    persist_session(&state, &user_id, &tokens, &req.session).await?;

    Ok(Json(AuthResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_in: tokens.expires_in,
        user: UserResponse {
            account_kind: response_account_kind(&state, &user_id).await?,
            id: user_id,
            email: user.0,
            username: user.1,
            display_name: user.2,
            avatar_url: user.3,
            role: user.4,
            totp_enabled: user.5 != 0,
        },
    }))
}

/// POST /api/v1/auth/2fa/totp/setup
pub(crate) async fn legacy_counter_produce(state:AppState,body:serde_json::Value)->StatusCode { match legacy_counter_original_login(State(state),Json(serde_json::from_value(body).unwrap())).await {Ok(r)=>r.into_response().status(),Err(e)=>e.into_response().status()} }
