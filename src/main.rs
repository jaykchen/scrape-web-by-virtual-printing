use axum::{
    extract::{Json, Query},
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use headless_chrome::{
    protocol::cdp::Target::CreateTarget, types::PrintToPdfOptions, Browser, LaunchOptions,
};
use html2text;
use http_req::request;
use pdfium_render::prelude::*;
use readah::readability::Readability;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use std::{net::SocketAddr, path::PathBuf};
use url::Url;

#[tokio::main]
async fn main() {
    // let addr = SocketAddr::from(([10, 0, 0, 75], 5000));
    let addr = SocketAddr::from(([10, 0, 0, 15], 3000));
    let app = Router::new()
        .route("/", get(handler))
        .route("/api", post(handle_post));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

#[derive(Debug, Serialize, Deserialize)]
struct Data {
    url: String,
}

async fn handle_post(data: Json<Data>) -> impl IntoResponse {
    println!("Received data: {:?}", data.url);

    if let Err(_) = Url::from_str(&data.url) {
        return Response::builder()
            .status(StatusCode::OK)
            .body("parse target url failure".to_string())
            .unwrap();
    } else {
        match text_to_use(&data.url).await {
            Ok(res) => {
                return Response::builder()
                    .status(StatusCode::OK)
                    .body(res)
                    .unwrap();
            }
            Err(_) => {
                return Response::builder()
                    .status(StatusCode::OK)
                    .body("failed to get text from webpage".to_string())
                    .unwrap();
            }
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct MyResponse {
    text: String,
}

async fn handler(params: Query<Params>) -> Json<MyResponse> {
    if let Some(url) = &params.url {
        if let Err(_) = Url::from_str(&url) {
            return Json(MyResponse {
                text: "parse target url failure".to_string(),
            });
        } else {
            match get_webpage_text_headless(&url).await {
                Ok(res) => {
                    return Json(MyResponse { text: res });
                }
                Err(_) => {
                    return Json(MyResponse {
                        text: "failed to get text from webpage".to_string(),
                    })
                }
            }
        }
    } else {
        return Json(MyResponse {
            text: "probably ill-formed request".to_string(),
        });
    }
}

#[derive(Debug, Deserialize)]
struct Params {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    url: Option<String>,
}

/// Serde deserialization decorator to map empty Strings to None
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s)
            .map_err(serde::de::Error::custom)
            .map(Some),
    }
}

async fn get_webpage_text_headless(url: &str) -> anyhow::Result<String> {
    // set the headless Chrome to open a webpage in portrait mode of certain width and height
    // here in an iPad resolution, is a way to pursuade webserver to send less non-essential
    // data, and make the virtual browser to show the central content, for websites
    // with responsive design, with less clutter
    let options = LaunchOptions {
        headless: true,
        window_size: Some((820, 1180)),
        path: Some(PathBuf::from_str("/usr/bin/google-chrome").unwrap()),
        ..Default::default()
    };

    let browser = Browser::new(options)?;

    let tab = browser.new_tab()?;

    tab.navigate_to(url)?;
    tab.wait_until_navigated()?;

    let pdf_options: Option<PrintToPdfOptions> = Some(PrintToPdfOptions {
        landscape: Some(false),
        display_header_footer: Some(false),
        print_background: Some(false),
        scale: Some(0.5),
        paper_width: Some(11.0),
        paper_height: Some(17.0),
        margin_top: Some(0.1),
        margin_bottom: Some(0.1),
        margin_left: Some(0.1),
        margin_right: Some(0.1),
        // page_ranges: Some("1-2".to_string()),
        ignore_invalid_page_ranges: Some(true),
        prefer_css_page_size: Some(false),
        transfer_mode: None,
        ..Default::default()
    });

    let pdf_data = tab.print_to_pdf(pdf_options)?;

    let pdf_as_vec = pdf_data.to_vec();
    //code below uses dynamically linked libpdfium.dylib on a M1 Mac
    //it takes some efforts to bind libpdfium on different platforms
    //please visit https://github.com/ajrcarey/pdfium-render/tree/master
    //for more details
    let text = Pdfium::new(
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
            "/home/ubuntu/pdfium/lib/",
            // "/Users/jaykchen/Downloads/pdfium-mac-arm64/lib/libpdfium.dylib",
        ))
        .or_else(|_| Pdfium::bind_to_system_library())?,
    )
    .load_pdf_from_byte_vec(pdf_as_vec, Some(""))?
    .pages()
    .iter()
    .map(|page| page.text().unwrap().all())
    .collect::<Vec<String>>()
    .join(" ");

    Ok(text)
}

pub async fn get_html_headless(url: &str) -> anyhow::Result<String> {
    let options = LaunchOptions {
        headless: true,
        window_size: Some((820, 1180)),
        path: Some(PathBuf::from_str("/usr/bin/google-chrome").unwrap()),
        ..Default::default()
    };

    let browser = Browser::new(options)?;
    let tab = browser.new_tab()?;
    tab.navigate_to(url)?;
    tab.wait_until_navigated()?;
    let text = tab.get_content()?;

    Ok(text)
}

pub async fn extract_article_text_from_html(url: &str, html_str: String) -> anyhow::Result<String> {
    let parsed_url = Url::parse(url)?;
    let scheme = parsed_url.scheme();
    let host = parsed_url.host_str().unwrap_or("");
    let base_url = Url::parse(&format!("{}://{}", scheme, host))?;

    let res = Readability::extract(&html_str, Some(base_url)).await?;
    let output = html2text::from_read(res.to_string().as_bytes(), 80);

    Ok(output)
}

pub async fn text_to_use(url: &str) -> anyhow::Result<String> {
    let pdf_text = get_webpage_text_headless(url).await?;
    let html_str = get_html_headless(url).await?;
    let readah_text = extract_article_text_from_html(url, html_str).await?;

    let readah_text_len = readah_text.split_whitespace().count();
    let pdf_text_len = pdf_text.split_whitespace().count();

    let lots_of_text_on_page = pdf_text_len > 999;
    let readah_sees_lots_of_texts = readah_text_len > 500;

    if lots_of_text_on_page && readah_sees_lots_of_texts {
        return Ok(readah_text.to_string());
    }

    Ok(pdf_text.to_string())
}
