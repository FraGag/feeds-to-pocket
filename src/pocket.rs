// Copyright 2015 The rust-pocket Developers
// Copyright 2016 Francis Gagné
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

// The code for this module
// is derived from the rust-pocket library,
// available at https://github.com/kstep/rust-pocket
// under the Apache 2.0 and MIT licenses.
// I made this derived version for two reasons:
// 1) The library would fail to decode some responses for the "add" endpoint.
//    I don't actually need to decode those responses,
//    so I removed that here.
// 2) The library provides methods for endpoints that I don't need,
//    but they're also presumably broken,
//    and I didn't feel like fixing and testing them.

use std::error::Error;
use std::convert::From;
use std::fmt;
use std::io::Error as IoError;
use std::io::Read;
use std::ops::{Deref, DerefMut};
use std::result::Result;

use hyper::Error as HyperError;
use mime::Mime;
use reqwest::{Client, Error as HttpError};
use reqwest::header::{self, ContentType, Header, Raw};
use reqwest::header::parsing::from_one_raw_str;
use serde::{Deserialize, Serialize, Serializer};
use serde_json;
use url::Url;
use url_serde;

#[derive(Debug)]
pub enum PocketError {
    Http(HttpError),
    Io(IoError),
    SerdeJson(serde_json::Error),
    Proto(u16, String)
}

pub type PocketResult<T> = Result<T, PocketError>;

impl From<serde_json::Error> for PocketError {
    fn from(err: serde_json::Error) -> PocketError {
        PocketError::SerdeJson(err)
    }
}

impl From<IoError> for PocketError {
    fn from(err: IoError) -> PocketError {
        PocketError::Io(err)
    }
}

impl From<HttpError> for PocketError {
    fn from(err: HttpError) -> PocketError {
        PocketError::Http(err)
    }
}

impl Error for PocketError {
    fn description(&self) -> &str {
        match *self {
            PocketError::Http(ref e) => e.description(),
            PocketError::Io(ref e) => e.description(),
            PocketError::SerdeJson(ref e) => e.description(),
            PocketError::Proto(..) => "protocol error"
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            PocketError::Http(ref e) => Some(e),
            PocketError::Io(ref e) => Some(e),
            PocketError::SerdeJson(ref e) => Some(e),
            PocketError::Proto(..) => None
        }
    }
}

impl fmt::Display for PocketError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match *self {
            PocketError::Http(ref e) => e.fmt(fmt),
            PocketError::Io(ref e) => e.fmt(fmt),
            PocketError::SerdeJson(ref e) => e.fmt(fmt),
            PocketError::Proto(ref code, ref msg) => fmt.write_str(&format!("{} (code {})", msg, code))
        }
    }
}

#[derive(Clone, Debug)]
struct XAccept(pub Mime);

impl Deref for XAccept {
    type Target = Mime;
    fn deref(&self) -> &Mime {
        &self.0
    }
}

impl DerefMut for XAccept {
    fn deref_mut(&mut self) -> &mut Mime {
        &mut self.0
    }
}

impl Header for XAccept {
    fn header_name() -> &'static str {
        "X-Accept"
    }

    fn parse_header(raw: &Raw) -> Result<XAccept, HyperError> {
        from_one_raw_str(raw).map(XAccept)
    }

    fn fmt_header(&self, fmt: &mut header::Formatter) -> fmt::Result {
        fmt.fmt_line(&self.0)
    }
}

#[derive(Clone, Debug)]
struct XError(String);
#[derive(Clone, Debug)]
struct XErrorCode(u16);

impl Header for XError {
    fn header_name() -> &'static str {
        "X-Error"
    }

    fn parse_header(raw: &Raw) -> Result<XError, HyperError> {
        from_one_raw_str(raw).map(XError)
    }

    fn fmt_header(&self, fmt: &mut header::Formatter) -> fmt::Result {
        fmt.fmt_line(&self.0)
    }
}

impl Header for XErrorCode {
    fn header_name() -> &'static str {
        "X-Error-Code"
    }

    fn parse_header(raw: &Raw) -> Result<XErrorCode, HyperError> {
        from_one_raw_str(raw).map(XErrorCode)
    }

    fn fmt_header(&self, fmt: &mut header::Formatter) -> fmt::Result {
        fmt.fmt_line(&self.0)
    }
}

pub struct Pocket {
    consumer_key: String,
    access_token: Option<String>,
    code: Option<String>,
    client: Client,
}

#[derive(Serialize)]
pub struct PocketOAuthRequest<'a> {
    consumer_key: &'a str,
    redirect_uri: &'a str,
    state: Option<&'a str>,
}

#[derive(Deserialize)]
pub struct PocketOAuthResponse {
    code: String,
}

#[derive(Serialize)]
pub struct PocketAuthorizeRequest<'a> {
    consumer_key: &'a str,
    code: &'a str,
}

#[derive(Deserialize)]
pub struct PocketAuthorizeResponse {
    access_token: String,
    username: String,
}

#[derive(Serialize)]
pub struct PocketAddRequest<'a> {
    consumer_key: &'a str,
    access_token: &'a str,
    #[serde(serialize_with = "serialize_url_ref")]
    url: &'a Url,
    title: Option<&'a str>,
    tags: Option<&'a str>,
    tweet_id: Option<&'a str>,
}

fn serialize_url_ref<S>(value: &&Url, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    url_serde::serialize(*value, serializer)
}

impl Pocket {
    pub fn new(consumer_key: &str, access_token: Option<&str>, client: Client) -> Pocket {
        Pocket {
            consumer_key: consumer_key.to_string(),
            access_token: access_token.map(|v| v.to_string()),
            code: None,
            client: client,
        }
    }

    #[inline]
    pub fn access_token(&self) -> Option<&str> {
        self.access_token.as_ref().map(|v| &**v)
    }

    fn request<Req: Serialize>(&self, url: &str, request: &Req) -> PocketResult<String> {
        let request = try!(serde_json::to_string(request));

        let app_json: Mime = "application/json".parse().unwrap();

        self.client.post(url)?
            .header(XAccept(app_json.clone()))
            .header(ContentType(app_json))
            .body(request)
            .send().map_err(From::from)
            .and_then(|mut r| match r.headers().get::<XErrorCode>().map(|v| v.0) {
                None => {
                    let mut out = String::new();
                    r.read_to_string(&mut out).map_err(From::from).map(|_| out)
                },
                Some(code) => Err(PocketError::Proto(code, r.headers().get::<XError>().map_or("unknown protocol error", |v| &*v.0).to_string())),
            })
    }

    pub fn get_auth_url(&mut self) -> PocketResult<Url> {
        // The final period is encoded as %2E
        // because on some terminals (e.g. Konsole),
        // the period is excluded from the URL
        // when you Ctrl+click it.
        const REDIRECT_URI: &'static str = "data:text/plain,Return%20to%20feeds-to-pocket%20and%20press%20Enter%20to%20finish%2E";

        let response = { // scope to release borrow on self
            let request = PocketOAuthRequest {
                consumer_key: &self.consumer_key,
                redirect_uri: REDIRECT_URI,
                state: None
            };

            self.request("https://getpocket.com/v3/oauth/request", &request)
        };

        response.and_then(|r| r.decode()).and_then(|r: PocketOAuthResponse| {
            let mut url = Url::parse("https://getpocket.com/auth/authorize").unwrap();
            url.query_pairs_mut().append_pair("request_token", &r.code).append_pair("redirect_uri", REDIRECT_URI);
            self.code = Some(r.code);
            Ok(url)
        })
    }

    pub fn authorize(&mut self) -> PocketResult<String> {
        {
            let request = PocketAuthorizeRequest {
                consumer_key: &self.consumer_key,
                code: self.code.as_ref().map(|v| &*v).unwrap()
            };

            self.request("https://getpocket.com/v3/oauth/authorize", &request)
        }.and_then(|r| r.decode()).and_then(|r: PocketAuthorizeResponse| {
            self.access_token = Some(r.access_token);
            Ok(r.username)
        })
    }

    pub fn add(&mut self, url: &Url, title: Option<&str>, tags: Option<&str>, tweet_id: Option<&str>) -> PocketResult<()> {
        let request = PocketAddRequest {
            consumer_key: &self.consumer_key,
            access_token: self.access_token.as_ref().unwrap(),
            url: url,
            title: title,
            tags: tags,
            tweet_id: tweet_id,
        };

        self.request("https://getpocket.com/v3/add", &request).map(|_| ())
    }
}

trait DecodeExt {
    fn decode<'a, Resp: Deserialize<'a>>(&'a self) -> PocketResult<Resp>;
}

impl DecodeExt for str {
    fn decode<'a, Resp: Deserialize<'a>>(&'a self) -> PocketResult<Resp> {
        serde_json::from_str::<Resp>(self).map_err(From::from)
    }
}
