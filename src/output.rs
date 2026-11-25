use std::io::{self, Write};

use anyhow::Result;
use serde::Serialize;

use crate::model::{App, AppOutput, ContentOutput, NamespacesOutput};
use crate::render;

pub fn json<T: Serialize>(value: &T) -> Result<()> {
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, value)?;
    writeln!(output)?;
    Ok(())
}

pub fn apps(apps: &[App]) -> Result<()> {
    write(&render::search(apps)?)
}

pub fn status<T: Serialize>(output: &T) -> Result<()> {
    write(&render::status(output)?)
}

pub fn set<T: Serialize>(output: &T) -> Result<()> {
    write(&render::set(output)?)
}

pub fn set_preview<T: Serialize>(output: &T) -> Result<()> {
    io::stderr()
        .lock()
        .write_all(render::set(output)?.as_bytes())?;
    Ok(())
}

pub fn namespace_update<T: Serialize>(output: &T) -> Result<()> {
    write(&render::namespace_update(output)?)
}

pub fn namespace_update_preview<T: Serialize>(output: &T) -> Result<()> {
    io::stderr()
        .lock()
        .write_all(render::namespace_update(output)?.as_bytes())?;
    Ok(())
}

pub fn release<T: Serialize>(output: &T) -> Result<()> {
    write(&render::release(output)?)
}

pub fn release_preview<T: Serialize>(output: &T) -> Result<()> {
    io::stderr()
        .lock()
        .write_all(render::release(output)?.as_bytes())?;
    Ok(())
}

pub fn app(output: &AppOutput) -> Result<()> {
    write(&render::app(output)?)
}

pub fn namespaces(output: &NamespacesOutput) -> Result<()> {
    write(&render::namespaces(output)?)
}

pub fn raw_namespace(output: &ContentOutput) -> Result<()> {
    write(&output.content)
}

pub fn namespace(output: &ContentOutput) -> Result<()> {
    write(&render::content_output(output)?)
}

fn write(rendered: &str) -> Result<()> {
    io::stdout().lock().write_all(rendered.as_bytes())?;
    Ok(())
}
