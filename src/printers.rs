use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::Serialize;
#[cfg(any(windows, test))]
use sha2::{Digest, Sha256};

use crate::config::AppConfig;

pub const CAPTURE_PRINTER_ID: &str = "capture:pdf";

#[derive(Clone, Debug, Serialize)]
pub struct PrinterInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub tested: bool,
    pub detail: String,
}

pub fn list_printers() -> Result<Vec<PrinterInfo>> {
    let mut printers = vec![PrinterInfo {
        id: CAPTURE_PRINTER_ID.to_owned(),
        name: "PrintLatch PDF Capture".to_owned(),
        kind: "capture".to_owned(),
        tested: true,
        detail:
            "Writes the accepted PDF to the local captures directory without contacting a printer."
                .to_owned(),
    }];
    printers.extend(platform_printers()?);
    Ok(printers)
}

pub fn printer_exists(id: &str) -> Result<bool> {
    Ok(list_printers()?.iter().any(|printer| printer.id == id))
}

pub fn submit(
    config: &AppConfig,
    job_id: &str,
    printer_id: &str,
    pdf_path: &Path,
    copies: u8,
) -> Result<String> {
    if printer_id == CAPTURE_PRINTER_ID {
        let destination = config.captures_dir().join(format!("{job_id}.pdf"));
        fs::copy(pdf_path, &destination)
            .with_context(|| format!("could not write {}", destination.display()))?;
        return Ok(format!(
            "Captured one PDF artifact at {} (requested copies: {copies})",
            destination.display()
        ));
    }
    platform_submit(printer_id, pdf_path, copies)
}

#[cfg(windows)]
fn platform_printers() -> Result<Vec<PrinterInfo>> {
    use winprint::printer::PrinterDevice;

    let devices = PrinterDevice::all().context("Windows printer enumeration failed")?;
    Ok(devices
        .into_iter()
        .map(|device| PrinterInfo {
            id: printer_id(device.name()),
            name: device.name().to_owned(),
            kind: if device.is_remote() {
                "windows_remote".to_owned()
            } else {
                "windows_local".to_owned()
            },
            tested: false,
            detail: "Discovered through the Windows print subsystem. Output depends on the installed driver and printer."
                .to_owned(),
        })
        .collect())
}

#[cfg(not(windows))]
fn platform_printers() -> Result<Vec<PrinterInfo>> {
    Ok(Vec::new())
}

#[cfg(windows)]
fn platform_submit(printer_id_value: &str, pdf_path: &Path, copies: u8) -> Result<String> {
    use winprint::printer::{FilePrinter, PrinterDevice, WinPdfPrinter};

    let device = PrinterDevice::all()
        .context("Windows printer enumeration failed")?
        .into_iter()
        .find(|device| printer_id(device.name()) == printer_id_value)
        .context("printer is no longer available")?;
    let name = device.name().to_owned();
    let printer = WinPdfPrinter::new(device);
    for copy in 0..copies {
        printer
            .print(pdf_path, Default::default())
            .with_context(|| format!("Windows rejected copy {}", copy + 1))?;
    }
    Ok(format!(
        "Submitted {copies} copy/copies to the Windows spooler for {name}. Physical output is not observable by PrintLatch."
    ))
}

#[cfg(not(windows))]
fn platform_submit(_printer_id: &str, _pdf_path: &Path, _copies: u8) -> Result<String> {
    bail!("physical printer submission is only supported on Windows in PrintLatch 0.1")
}

#[cfg(any(windows, test))]
fn printer_id(name: &str) -> String {
    let digest = hex::encode(Sha256::digest(name.as_bytes()));
    format!("win:{}", &digest[..24])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printer_ids_do_not_include_driver_control_characters() {
        let id = printer_id("printer & calc.exe\r\n");
        assert!(id.starts_with("win:"));
        assert!(
            id.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == ':')
        );
    }
}
