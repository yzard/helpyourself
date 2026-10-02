//! Bounded PDF text evidence. Images are always retained as the recognition input.
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use tokio::io::AsyncReadExt;

pub const MAX_TEXT_BYTES: usize = 32 * 1024;
pub const MAX_WORDS: usize = 4096;
const MAX_XML_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextWord {
    pub text: String,
    pub bounding_box: [f64; 4],
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextLayer {
    pub status: String,
    pub text: String,
    pub words: Vec<TextWord>,
}
#[derive(Clone, Serialize)]
pub struct PageEvidence {
    pub version: &'static str,
    pub image_kind: &'static str,
    pub text_layer: Option<TextLayer>,
    pub raw_bbox_xml: Option<String>,
    pub error_code: Option<&'static str>,
}
impl PageEvidence {
    pub fn image() -> Self {
        Self {
            version: "document-evidence-v1",
            image_kind: "original_or_processing_copy",
            text_layer: None,
            raw_bbox_xml: None,
            error_code: None,
        }
    }
    fn failure(status: &str, code: &'static str, xml: Option<String>) -> Self {
        Self {
            version: "document-evidence-v1",
            image_kind: "pdf_render_2400",
            text_layer: Some(TextLayer {
                status: status.into(),
                text: String::new(),
                words: vec![],
            }),
            raw_bbox_xml: xml,
            error_code: Some(code),
        }
    }
}

pub fn parse_pdf_layer(xml: String) -> PageEvidence {
    match parse_layer(&xml) {
        Ok(layer) => PageEvidence {
            version: "document-evidence-v1",
            image_kind: "pdf_render_2400",
            text_layer: Some(layer),
            raw_bbox_xml: Some(xml),
            error_code: None,
        },
        Err("text_layer_limit") => {
            PageEvidence::failure("limit_exceeded", "text_layer_limit", Some(xml))
        }
        Err(_) => PageEvidence::failure("unavailable", "text_layer_invalid", Some(xml)),
    }
}
fn parse_layer(xml: &str) -> Result<TextLayer, &'static str> {
    if xml.len() > MAX_XML_BYTES {
        return Err("text_layer_limit");
    }
    // Poppler emits a standard XHTML doctype. The parser never fetches external entities.
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: 100_000,
        entity_resolver: None,
    };
    let document =
        roxmltree::Document::parse_with_options(xml, options).map_err(|_| "text_layer_invalid")?;
    let pages = document
        .descendants()
        .filter(|node| node.has_tag_name("page"))
        .collect::<Vec<_>>();
    if pages.len() != 1 {
        return Err("text_layer_invalid");
    }
    let page = pages[0];
    let number = |node: roxmltree::Node, name: &str| -> Result<f64, &'static str> {
        let value = node
            .attribute(name)
            .ok_or("text_layer_invalid")?
            .parse::<f64>()
            .map_err(|_| "text_layer_invalid")?;
        if !value.is_finite() {
            return Err("text_layer_invalid");
        }
        Ok(value)
    };
    let width = number(page, "width")?;
    let height = number(page, "height")?;
    if width <= 0.0 || height <= 0.0 {
        return Err("text_layer_invalid");
    }
    let mut words = Vec::new();
    let mut lines = Vec::new();
    let mut total = 0;
    for line in page.descendants().filter(|node| node.has_tag_name("line")) {
        let mut pieces = Vec::new();
        for word in line.children().filter(|node| node.has_tag_name("word")) {
            let text = word.text().ok_or("text_layer_invalid")?.to_owned();
            let bounds = [
                number(word, "xMin")? / width,
                number(word, "yMin")? / height,
                number(word, "xMax")? / width,
                number(word, "yMax")? / height,
            ];
            if text.len() > 4096
                || bounds.iter().any(|value| !(0.0..=1.0).contains(value))
                || bounds[0] >= bounds[2]
                || bounds[1] >= bounds[3]
            {
                return Err("text_layer_invalid");
            }
            total += text.len() + 1;
            if total > MAX_TEXT_BYTES || words.len() >= MAX_WORDS {
                return Err("text_layer_limit");
            }
            pieces.push(text.clone());
            words.push(TextWord {
                text,
                bounding_box: bounds,
            });
        }
        if !pieces.is_empty() {
            lines.push(pieces.join(" "));
        }
    }
    if page
        .descendants()
        .filter(|node| node.has_tag_name("word"))
        .count()
        != words.len()
    {
        return Err("text_layer_invalid");
    }
    let text = lines.join("\n");
    let status = if text.trim().is_empty() {
        "empty"
    } else {
        "available"
    };
    Ok(TextLayer {
        status: status.into(),
        text,
        words,
    })
}

pub async fn pdf_evidence(
    cpu: &crate::execution::CpuExecutor,
    source: &Path,
    page: i64,
) -> Result<PageEvidence, AppError> {
    let mut command = tokio::process::Command::new("pdftotext");
    command
        .args([
            "-f",
            &page.to_string(),
            "-l",
            &page.to_string(),
            "-bbox-layout",
            "-enc",
            "UTF-8",
        ])
        .arg(source)
        .arg("-")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            return Ok(PageEvidence::failure(
                "unavailable",
                "text_layer_start_failed",
                None,
            ));
        }
    };
    let stdout = child.stdout.take().ok_or(AppError::Internal)?;
    let mut bytes = Vec::new();
    let reading = async {
        stdout
            .take((MAX_XML_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > MAX_XML_BYTES {
            child.kill().await?;
            return Ok::<_, std::io::Error>(None);
        }
        Ok(Some(child.wait().await?))
    };
    match tokio::time::timeout(Duration::from_secs(30), reading).await {
        Err(_) => {
            child.kill().await?;
            Ok(PageEvidence::failure(
                "unavailable",
                "text_layer_timeout",
                None,
            ))
        }
        Ok(Err(_)) => {
            child.kill().await?;
            Ok(PageEvidence::failure(
                "unavailable",
                "text_layer_io_failed",
                None,
            ))
        }
        Ok(Ok(None)) => Ok(PageEvidence::failure(
            "limit_exceeded",
            "text_layer_output_limit",
            None,
        )),
        Ok(Ok(Some(status))) => {
            let xml = String::from_utf8(bytes).ok();
            if !status.success() {
                return Ok(PageEvidence::failure(
                    "unavailable",
                    "text_layer_extract_failed",
                    xml,
                ));
            }
            match xml {
                Some(xml) => cpu.run_background(move || Ok(parse_pdf_layer(xml))).await,
                None => Ok(PageEvidence::failure(
                    "unavailable",
                    "text_layer_encoding_invalid",
                    None,
                )),
            }
        }
    }
}
