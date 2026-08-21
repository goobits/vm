fn main() -> Result<(), Box<dyn std::error::Error>> {
    let prompt = std::env::args().nth(1).unwrap_or_default();
    if prompt.to_ascii_lowercase().contains("username") {
        println!(
            "{}",
            std::env::var("PKG_WORK_GIT_USERNAME").unwrap_or_else(|_| "x-access-token".into())
        );
        return Ok(());
    }

    let path = std::env::var("PKG_WORK_GIT_TOKEN_FILE")?;
    print!("{}", std::fs::read_to_string(path)?.trim());
    Ok(())
}
