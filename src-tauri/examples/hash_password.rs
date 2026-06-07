use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher,
};

fn escape_for_docker_compose_env(value: &str) -> String {
    value.replace('$', "$$")
}

fn main() {
    let password = std::env::args().nth(1).unwrap_or_else(|| "changeme".to_string());
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("hash password");
    let hash_str = hash.to_string();

    println!("原始哈希（仅用于调试，勿泄露）：");
    println!("{hash_str}");
    println!();
    println!("写入 .env 请用下面这一行（Docker Compose 会把 $ 展开，必须写成 $$）：");
    println!("APP_PASSWORD_HASH={}", escape_for_docker_compose_env(&hash_str));
    println!();
    println!("本地 shell 临时 export 请用单引号：");
    println!("export APP_PASSWORD_HASH='{hash_str}'");
}
