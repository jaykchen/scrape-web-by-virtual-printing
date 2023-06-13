use anyhow::Result;
use axum::{extract::Query, response::Json, routing::get, Router};
use headless_chrome::{types::PrintToPdfOptions, Browser, LaunchOptions};
use pdfium_render::prelude::*;
use serde::{de, Deserialize, Deserializer};
use std::{fmt, str::FromStr};
use url::Url;

#[tokio::main]
async fn main() {
    // axum::Server::bind(&"10.0.0.75:5000".parse().unwrap())
    axum::Server::bind(&"10.0.0.15:3000".parse().unwrap())
        .serve(app().into_make_service())
        .await
        .unwrap();
}

fn app() -> Router {
    Router::new().route("/", get(handler))
}

#[derive(Debug, serde::Serialize)]
struct Response {
    text: String,
}

async fn handler(Query(params): Query<Params>) -> Json<Response> {
    if let Some(url) = params.url {
        if let Err(_) = Url::from_str(&url) {
            return Json(Response {
                text: "parse target url failure".to_string(),
            });
        } else {
            match get_text_headless(&url).await {
                Ok(res) => {
                    return Json(Response { text: res });
                }

                Err(_) => {
                    return Json(Response {
                        text: "failed to get text from webpage".to_string(),
                    })
                }
            }
        }
    } else {
        return Json(Response {
            text: "probably ill-formed request".to_string(),
        });
    }
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Params {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    url: Option<String>,
}

/// Serde deserialization decorator to map empty Strings to None,
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s).map_err(de::Error::custom).map(Some),
    }
}

async fn get_text_headless(url: &str) -> anyhow::Result<String> {
    // set the headless Chrome to open a webpage in portrait mode of certain width and height
    // here in an iPad resolution, is a way to pursuade webserver to send less non-essential
    // data, and make the virtual browser to show the central content, for websites
    // with responsive design, with less clutter
    let options = LaunchOptions {
        headless: true,
        window_size: Some((820, 1180)),
        ..Default::default()
    };

    let browser = Browser::new(options)?;

    let tab = browser.new_tab()?;

    tab.navigate_to(url)?;
    tab.wait_until_navigated();

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
        page_ranges: Some("1-2".to_string()),
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
