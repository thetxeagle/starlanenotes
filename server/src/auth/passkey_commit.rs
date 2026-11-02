// Atomic credential transactions. Cryptographic refusal retains one-time retirement;
// database/readback failures roll back the challenge and every dependent write.
#[derive(Deserialize)]
struct PersistedPasskeyCounter{cred:PersistedCredentialCounter}
#[derive(Deserialize)]
struct PersistedCredentialCounter{counter:u32}
type PasskeyCommitRow=(String,String,String,String,String,i64,String,Option<String>);
enum PasskeyTake<T>{Ready(Option<String>,T,String),Retired(AppError)}
async fn passkey_row(conn:&mut sqlx::AnyConnection,id:&str)->Result<Option<PasskeyCommitRow>,AppError>{
 sqlx::query_as("SELECT id,user_id,credential_id,public_key,name,sign_count,created_at,last_used_at FROM passkeys WHERE id=$1")
 .bind(id).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn take_passkey_in<T:for<'de> Deserialize<'de>>(conn:&mut sqlx::AnyConnection,id:&str,kind_expected:&str,owner:Option<&str>)->Result<PasskeyTake<T>,AppError>{
 let row:Option<(Option<String>,Option<String>,String,String,String,String)>=sqlx::query_as("SELECT user_id,email,kind,state_json,expires_at,created_at FROM passkey_challenges WHERE id=$1")
 .bind(id).fetch_optional(&mut*conn).await.map_err(AppError::internal)?;
 let (user,email,kind,state,expiry,created)=row.ok_or_else(||AppError::BadRequest("Passkey challenge is invalid or expired".into()))?;
 if let(Some(owner),Some(actual))=(owner,user.as_deref()) {if owner!=actual{return Err(AppError::Unauthorized("Passkey registration challenge does not belong to this account".into()));}}
 let now=chrono::Utc::now().to_rfc3339();
 let affected=sqlx::query("DELETE FROM passkey_challenges WHERE id=$1 AND kind=$2 AND state_json=$3 AND expires_at=$4 AND (user_id=$5 OR (user_id IS NULL AND $5 IS NULL)) AND (email=$6 OR (email IS NULL AND $6 IS NULL)) AND created_at=$7")
 .bind(id).bind(&kind).bind(&state).bind(&expiry).bind(&user).bind(&email).bind(&created).execute(&mut*conn).await.map_err(AppError::internal)?.rows_affected();
 if affected!=1{return Err(AppError::BadRequest("Passkey challenge is invalid or expired".into()));}
 if expiry<=now{return Ok(PasskeyTake::Retired(AppError::BadRequest("Passkey challenge is invalid or expired".into())));}
 if kind!=kind_expected{return Ok(PasskeyTake::Retired(AppError::BadRequest("Passkey challenge type does not match this request".into())));}
 match serde_json::from_str(&state){Ok(state)=>Ok(PasskeyTake::Ready(user,state,expiry)),Err(e)=>Ok(PasskeyTake::Retired(AppError::internal(e)))}
}
async fn passkey_consumed(conn:&mut sqlx::AnyConnection,id:&str,expiry:&str)->Result<(),AppError>{
 let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM passkey_challenges WHERE id=$1").bind(id).fetch_one(conn).await.map_err(AppError::internal)?;
 if count!=0 || !chrono::DateTime::parse_from_rfc3339(expiry).is_ok_and(|e|e>chrono::Utc::now()) {return Err(AppError::Conflict("Passkey challenge changed".into()));}Ok(())
}
async fn passkey_actor(conn:&mut sqlx::AnyConnection,auth:&AuthUser)->Result<AccountLifecycleUser,AppError>{
 let actor=crate::spaces::access::require_current_human_identity(conn,auth).await?;
 account_lifecycle_user(conn,&actor).await?.ok_or_else(||AppError::Unauthorized("Account unavailable".into()))
}
async fn commit_passkey_registration(state:&AppState,auth:&AuthUser,req:PasskeyRegisterFinishRequest)->Result<(StatusCode,Json<PasskeyResponse>),AppError>{
 let mut tx=state.db.begin().await.map_err(AppError::internal)?;
 crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
 let actor=passkey_actor(&mut tx,auth).await?;
 let(user,registration,expiry)=match take_passkey_in::<PasskeyRegistration>(&mut tx,&req.challenge_id,"registration",Some(&actor.id)).await? {
  PasskeyTake::Ready(u,s,e)=>(u,s,e),PasskeyTake::Retired(e)=>{tx.commit().await.map_err(AppError::internal)?;return Err(e);}
 };
 if let Err(e)=validate_passkey_registration_challenge_user(user,&actor.id){tx.commit().await.map_err(AppError::internal)?;return Err(e);}
 let credential:RegisterPublicKeyCredential=match serde_json::from_value(req.credential){Ok(c)=>c,Err(_)=>{tx.commit().await.map_err(AppError::internal)?;return Err(AppError::BadRequest("Passkey registration response is malformed".into()));}};
 let webauthn=webauthn_for_state(state)?;
 let passkey=match webauthn.finish_passkey_registration(&credential,&registration){Ok(p)=>p,Err(e)=>{tx.commit().await.map_err(AppError::internal)?;return Err(AppError::Unauthorized(format!("Passkey registration failed: {e}")));}};
 let credential_id=credential_id_to_string(passkey.cred_id())?;
 let existing:Option<String>=sqlx::query_scalar("SELECT id FROM passkeys WHERE credential_id=$1").bind(&credential_id).fetch_optional(&mut*tx).await.map_err(AppError::internal)?;
 if existing.is_some(){tx.commit().await.map_err(AppError::internal)?;return Err(AppError::Conflict("This passkey is already registered.".into()));}
 let id=Uuid::new_v4().to_string();let name=req.name.unwrap_or_else(||"Passkey".into()).trim().chars().take(80).collect::<String>();let name=if name.is_empty(){"Passkey".into()}else{name};
 let now=chrono::Utc::now().to_rfc3339();let stored_created=chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();let expected:PasskeyCommitRow=(id.clone(),actor.id.clone(),credential_id,serde_json::to_string(&passkey).map_err(AppError::internal)?,name.clone(),0,stored_created,None);
 if sqlx::query("INSERT INTO passkeys(id,user_id,credential_id,public_key,name,sign_count,created_at,last_used_at) VALUES($1,$2,$3,$4,$5,$6,$7,NULL)")
 .bind(&expected.0).bind(&expected.1).bind(&expected.2).bind(&expected.3).bind(&expected.4).bind(expected.5).bind(&expected.6).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Passkey registration changed".into()));}
 if passkey_row(&mut tx,&id).await?!=Some(expected) || passkey_actor(&mut tx,auth).await?!=actor {return Err(AppError::Conflict("Passkey registration source changed".into()));}
 passkey_consumed(&mut tx,&req.challenge_id,&expiry).await?;tx.commit().await.map_err(AppError::internal)?;
 crate::audit::log_action(&state.db,Some(&actor.id),None,"user.passkey_create",Some("passkey"),Some(&id)).await;
 Ok((StatusCode::CREATED,Json(PasskeyResponse{id,name,created_at:now,last_used_at:None})))
}
async fn commit_passkey_login(state:&AppState,req:PasskeyLoginFinishRequest)->Result<AuthResponse,AppError>{
 let mut tx=state.db.begin().await.map_err(AppError::internal)?;
 crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
 let(user,authentication,expiry)=match take_passkey_in::<PasskeyAuthentication>(&mut tx,&req.challenge_id,"authentication",None).await?{
  PasskeyTake::Ready(u,s,e)=>(u,s,e),PasskeyTake::Retired(e)=>{tx.commit().await.map_err(AppError::internal)?;return Err(e);}
 };
 let Some(user_id)=user else{tx.commit().await.map_err(AppError::internal)?;return Err(AppError::BadRequest("Passkey challenge is missing account state".into()));};
 let credential:PublicKeyCredential=match serde_json::from_value(req.credential){Ok(c)=>c,Err(_)=>{tx.commit().await.map_err(AppError::internal)?;return Err(AppError::BadRequest("Passkey sign-in response is malformed".into()));}};
 let webauthn=webauthn_for_state(state)?;
 let result=match webauthn.finish_passkey_authentication(&credential,&authentication){Ok(r)=>r,Err(e)=>{tx.commit().await.map_err(AppError::internal)?;return Err(AppError::Unauthorized(format!("Passkey sign-in failed: {e}")));}};
 let credential_id=credential_id_to_string(result.cred_id())?;
 let id:Option<String>=sqlx::query_scalar("SELECT id FROM passkeys WHERE user_id=$1 AND credential_id=$2").bind(&user_id).bind(&credential_id).fetch_optional(&mut*tx).await.map_err(AppError::internal)?;
 let Some(id)=id else{tx.commit().await.map_err(AppError::internal)?;return Err(AppError::Unauthorized("Passkey is not registered for this account".into()));};
 let source=passkey_row(&mut tx,&id).await?.ok_or_else(||AppError::Unauthorized("Passkey is not registered for this account".into()))?;
 let user=account_lifecycle_user(&mut tx,&user_id).await?.filter(|u|u.is_active==1&&matches!(u.account_kind.as_str(),"member"|"guest")).ok_or_else(||AppError::Unauthorized("Account unavailable".into()))?;
 let counter=i64::from(result.counter());
 let mut passkey:Passkey=serde_json::from_str(&source.3).map_err(AppError::internal)?;
 // The challenge can predate another successful login. Fence against the held
 // CURRENT native key as well as SQL, including legacy counter drift.
 let native:PersistedPasskeyCounter=serde_json::from_value(serde_json::to_value(&passkey).map_err(AppError::internal)?).map_err(AppError::internal)?;
 if source.5<0{return Err(AppError::Conflict("Passkey counter state is invalid".into()));}
 let held_counter=source.5.max(i64::from(native.cred.counter));
 if (counter!=0||held_counter!=0)&&counter<=held_counter {tx.commit().await.map_err(AppError::internal)?;return Err(AppError::Unauthorized("Passkey counter is stale".into()));}
 let mut expected=source.clone();
 if passkey.update_credential(&result).is_none(){return Err(AppError::Conflict("Passkey credential changed".into()));}
 expected.3=serde_json::to_string(&passkey).map_err(AppError::internal)?;expected.5=counter;
 expected.7=Some(chrono::Utc::now().to_rfc3339());
 if sqlx::query("UPDATE passkeys SET public_key=$1,sign_count=$2,last_used_at=$3 WHERE id=$4 AND user_id=$5 AND credential_id=$6 AND public_key=$7 AND sign_count=$8 AND (last_used_at=$9 OR (last_used_at IS NULL AND $9 IS NULL))")
 .bind(&expected.3).bind(expected.5).bind(&expected.7).bind(&source.0).bind(&source.1).bind(&source.2).bind(&source.3).bind(source.5).bind(&source.7).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Passkey source changed".into()));}
 let kind=normalized_session_kind(req.session.session_kind.as_deref());let platform=normalized_client_platform(req.session.client_platform.as_deref(),&kind);
 let requested=normalized_device_name(req.session.device_name.as_deref())?.unwrap_or_else(||default_session_name(&platform).into());
 let mut sessions=initial_session_row(&mut tx,&user_id).await?;let name=unique_session_name_from_rows(&sessions.iter().map(|r|(r.3.clone(),)).collect::<Vec<_>>(),&requested)?;
 let tokens=create_session_tokens(&user_id,&user.email,&user.role,&state.jwt_secret,&req.session)?;
 let now=chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
 let session:InitialSessionRow=(tokens.session_id.clone(),user_id.clone(),hash_refresh_token(&tokens.refresh_token),Some(name),None,None,now.clone(),session_expiry_at(&kind),now,kind,platform);
 if sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,device_name,ip_address,user_agent,last_seen_at,expires_at,created_at,session_kind,client_platform) VALUES($1,$2,$3,$4,NULL,NULL,$5,$6,$7,$8,$9)")
 .bind(&session.0).bind(&session.1).bind(&session.2).bind(&session.3).bind(&session.6).bind(&session.7).bind(&session.8).bind(&session.9).bind(&session.10).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Passkey session changed".into()));}
 sessions.push(session);sessions.sort_by(|a,b|a.0.cmp(&b.0));
 if passkey_row(&mut tx,&id).await?!=Some(expected)||account_lifecycle_user(&mut tx,&user_id).await?!=Some(user.clone())||initial_session_row(&mut tx,&user_id).await?!=sessions{return Err(AppError::Conflict("Passkey login source changed".into()));}
 passkey_consumed(&mut tx,&req.challenge_id,&expiry).await?;tx.commit().await.map_err(AppError::internal)?;
 Ok(AuthResponse{access_token:tokens.access_token,refresh_token:tokens.refresh_token,expires_in:tokens.expires_in,user:UserResponse{account_kind:user.account_kind,id:user_id,email:user.email,username:user.username,display_name:user.display_name,avatar_url:user.avatar_url,role:user.role,totp_enabled:user.totp_enabled!=0}})
}
async fn commit_passkey_delete(state:&AppState,auth:&AuthUser,id:&str)->Result<StatusCode,AppError>{
 let mut tx=state.db.begin().await.map_err(AppError::internal)?;crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
 let actor=passkey_actor(&mut tx,auth).await?;
 let source=passkey_row(&mut tx,id).await?.filter(|r|r.1==actor.id).ok_or_else(||AppError::NotFound("Passkey not found".into()))?;
 if sqlx::query("DELETE FROM passkeys WHERE id=$1 AND user_id=$2 AND credential_id=$3 AND public_key=$4 AND name=$5 AND sign_count=$6 AND created_at=$7 AND (last_used_at=$8 OR (last_used_at IS NULL AND $8 IS NULL))")
 .bind(&source.0).bind(&source.1).bind(&source.2).bind(&source.3).bind(&source.4).bind(source.5).bind(&source.6).bind(&source.7).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Passkey changed".into()));}
 if passkey_row(&mut tx,id).await?.is_some()||passkey_actor(&mut tx,auth).await?!=actor{return Err(AppError::Conflict("Passkey deletion source changed".into()));}
 tx.commit().await.map_err(AppError::internal)?;crate::audit::log_action(&state.db,Some(&actor.id),None,"user.passkey_delete",Some("passkey"),Some(id)).await;Ok(StatusCode::NO_CONTENT)
}
