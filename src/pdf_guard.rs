use std::io::Read;

use anyhow::{Context, Result, bail};
use flate2::read::ZlibDecoder;
use lopdf::{Document, Object};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::MAX_JOB_BYTES;

const MAX_PAGES: usize = 100;
const MAX_OBJECTS: usize = 20_000;
const MAX_DECODED_STREAM_BYTES: u64 = 100 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 100_000_000;
const FORBIDDEN_NAMES: [&[u8]; 9] = [
    b"/JavaScript",
    b"/JS",
    b"/Launch",
    b"/OpenAction",
    b"/EmbeddedFile",
    b"/Filespec",
    b"/RichMedia",
    b"/XFA",
    b"/AcroForm",
];

#[derive(Clone, Debug, Serialize)]
pub struct PdfReport {
    pub byte_count: u64,
    pub page_count: u32,
    pub object_count: u32,
    pub sha256: String,
}

pub fn validate_pdf(bytes: &[u8], declared_mime: &str) -> Result<PdfReport> {
    if declared_mime != "application/pdf" {
        bail!("MIME type must be application/pdf");
    }
    if bytes.is_empty() {
        bail!("PDF is empty");
    }
    if bytes.len() > MAX_JOB_BYTES {
        bail!("PDF exceeds the 10 MiB V1 limit");
    }
    let header_window = &bytes[..bytes.len().min(1_024)];
    if !header_window.windows(5).any(|window| window == b"%PDF-") {
        bail!("PDF signature is missing");
    }
    let trailer_start = bytes.len().saturating_sub(2_048);
    if !bytes[trailer_start..]
        .windows(5)
        .any(|window| window == b"%%EOF")
    {
        bail!("PDF end marker is missing");
    }
    for forbidden in FORBIDDEN_NAMES {
        if bytes
            .windows(forbidden.len())
            .any(|window| window == forbidden)
        {
            bail!(
                "active or embedded PDF feature {} is not accepted",
                String::from_utf8_lossy(forbidden)
            );
        }
    }

    let document = Document::load_mem(bytes).context("PDF parser rejected the document")?;
    if document.trailer.get(b"Encrypt").is_ok() {
        bail!("encrypted PDFs are not supported in V1");
    }
    if document.objects.len() > MAX_OBJECTS {
        bail!("PDF contains too many objects");
    }
    let pages = document.get_pages();
    if pages.is_empty() {
        bail!("PDF contains no pages");
    }
    if pages.len() > MAX_PAGES {
        bail!("PDF exceeds the 100-page V1 limit");
    }

    let mut decoded_total = 0_u64;
    let mut image_pixels_total = 0_u64;
    for object in document.objects.values() {
        let Object::Stream(stream) = object else {
            continue;
        };
        if dictionary_name(&stream.dict, b"Subtype") == Some(b"Image".as_slice()) {
            let width = dictionary_positive_integer(&stream.dict, b"Width").unwrap_or(0);
            let height = dictionary_positive_integer(&stream.dict, b"Height").unwrap_or(0);
            image_pixels_total = image_pixels_total
                .checked_add(width.saturating_mul(height))
                .context("image dimensions overflow")?;
            if image_pixels_total > MAX_IMAGE_PIXELS {
                bail!("PDF image dimensions exceed the 100-megapixel safety limit");
            }
        }

        let compressed_len = u64::try_from(stream.content.len()).context("stream is too large")?;
        if has_filter(&stream.dict, b"FlateDecode") {
            let mut decoder =
                ZlibDecoder::new(stream.content.as_slice()).take(MAX_DECODED_STREAM_BYTES + 1);
            let copied = std::io::copy(&mut decoder, &mut std::io::sink())
                .context("invalid FlateDecode stream")?;
            decoded_total = decoded_total
                .checked_add(copied)
                .context("decoded stream sizes overflow")?;
        } else {
            decoded_total = decoded_total
                .checked_add(compressed_len)
                .context("stream sizes overflow")?;
        }
        if decoded_total > MAX_DECODED_STREAM_BYTES {
            bail!("PDF streams exceed the 100 MiB decoded safety limit");
        }
    }

    Ok(PdfReport {
        byte_count: u64::try_from(bytes.len()).context("file size is out of range")?,
        page_count: u32::try_from(pages.len()).context("page count is out of range")?,
        object_count: u32::try_from(document.objects.len())
            .context("object count is out of range")?,
        sha256: hex::encode(Sha256::digest(bytes)),
    })
}

fn dictionary_name<'a>(dictionary: &'a lopdf::Dictionary, key: &[u8]) -> Option<&'a [u8]> {
    dictionary.get(key).ok().and_then(|value| match value {
        Object::Name(name) => Some(name.as_slice()),
        _ => None,
    })
}

fn dictionary_positive_integer(dictionary: &lopdf::Dictionary, key: &[u8]) -> Option<u64> {
    dictionary
        .get(key)
        .ok()
        .and_then(|value| value.as_i64().ok())
        .and_then(|value| u64::try_from(value).ok())
}

fn has_filter(dictionary: &lopdf::Dictionary, filter: &[u8]) -> bool {
    match dictionary.get(b"Filter") {
        Ok(Object::Name(name)) => name.as_slice() == filter,
        Ok(Object::Array(filters)) => filters.iter().any(|value| match value {
            Object::Name(name) => name.as_slice() == filter,
            _ => false,
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};

    fn minimal_pdf() -> Vec<u8> {
        br#"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>
endobj
xref
0 4
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
trailer
<< /Size 4 /Root 1 0 R >>
startxref
186
%%EOF
"#
        .to_vec()
    }

    #[test]
    fn accepts_small_static_pdf() {
        let report = validate_pdf(&minimal_pdf(), "application/pdf").expect("valid PDF");
        assert_eq!(report.page_count, 1);
        assert_eq!(report.sha256.len(), 64);
    }

    #[test]
    fn release_sample_is_a_valid_static_pdf() {
        let report = validate_pdf(
            include_bytes!("../docs/assets/sample.pdf"),
            "application/pdf",
        )
        .expect("release sample PDF");
        assert_eq!(report.page_count, 1);
    }

    #[test]
    fn rejects_false_mime_and_zip_magic() {
        assert!(validate_pdf(&minimal_pdf(), "application/octet-stream").is_err());
        assert!(validate_pdf(b"PK\x03\x04not a pdf%%EOF", "application/pdf").is_err());
    }

    #[test]
    fn rejects_active_content() {
        let mut pdf = minimal_pdf();
        let marker = pdf
            .windows(5)
            .position(|window| window == b"%%EOF")
            .expect("EOF exists");
        pdf.splice(marker..marker, b"/JavaScript ".iter().copied());
        assert!(validate_pdf(&pdf, "application/pdf").is_err());
    }

    #[test]
    fn rejects_pdf_image_dimension_bomb() {
        let mut document = Document::load_mem(&minimal_pdf()).expect("base PDF");
        document.objects.insert(
            (4, 0),
            Object::Stream(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => 50_000,
                    "Height" => 50_000,
                    "ColorSpace" => "DeviceRGB",
                    "BitsPerComponent" => 8,
                },
                Vec::new(),
            )),
        );
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).expect("serialize bomb PDF");

        let error = validate_pdf(&bytes, "application/pdf").expect_err("bomb must be rejected");
        assert!(
            error.to_string().contains("100-megapixel"),
            "unexpected error: {error:#}"
        );
    }
}
