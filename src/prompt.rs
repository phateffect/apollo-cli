use std::io::{self, IsTerminal, Write};

use anyhow::{Result, bail};
use zeroize::Zeroizing;

pub fn required(label: &str) -> Result<String> {
    eprint!("{label}: ");
    io::stderr().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        bail!("input ended before {label} was provided");
    }
    let value = input.trim().to_owned();
    if value.is_empty() {
        bail!("{label} cannot be empty");
    }
    Ok(value)
}

pub fn password() -> Result<Zeroizing<String>> {
    let password = if io::stdin().is_terminal() {
        rpassword::prompt_password("Password: ")?
    } else {
        eprint!("Password: ");
        io::stderr().flush()?;
        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            bail!("input ended before Password was provided");
        }
        let length = input.trim_end_matches(['\r', '\n']).len();
        input.truncate(length);
        input
    };
    let password = Zeroizing::new(password);
    if password.is_empty() {
        bail!("Password cannot be empty");
    }
    Ok(password)
}

pub fn confirm(message: &str) -> Result<bool> {
    eprint!("{message} [y/N]: ");
    io::stderr().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        eprintln!();
        return Ok(false);
    }
    let confirmed = matches!(input.trim().to_ascii_lowercase().as_str(), "y" | "yes");
    eprintln!();
    Ok(confirmed)
}
