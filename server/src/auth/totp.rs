use rand::Rng;
use totp_rs::{Algorithm, Secret, TOTP};

pub struct TotpSetup {
    pub secret: String,
    pub qr_code_url: String,
    pub recovery_codes: Vec<String>,
}

pub fn generate_totp_setup(email: &str) -> Result<TotpSetup, anyhow::Error> {
    let secret = Secret::generate_secret();
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().map_err(|e| anyhow::anyhow!("{}", e))?,
        Some("Starlane Notes".to_string()),
        email.to_string(),
    )?;

    let qr_code_url = totp.get_url();
    let recovery_codes = generate_recovery_codes(8);

    Ok(TotpSetup {
        secret: secret.to_encoded().to_string(),
        qr_code_url,
        recovery_codes,
    })
}

pub fn verify_totp_code(secret: &str, code: &str) -> Result<bool, anyhow::Error> {
    let secret = Secret::Encoded(secret.to_string());
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().map_err(|e| anyhow::anyhow!("{}", e))?,
        None,
        String::new(),
    )?;

    Ok(totp.check_current(code)?)
}

fn generate_recovery_codes(count: usize) -> Vec<String> {
    let mut rng = rand::thread_rng();
    (0..count)
        .map(|_| {
            let code: u32 = rng.gen_range(10000000..99999999);
            format!("{}-{}", &code.to_string()[..4], &code.to_string()[4..])
        })
        .collect()
}
