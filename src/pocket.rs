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

use std::convert::From;
use std::error::Error;
use std::fmt;
use std::io::Error as IoError;
use std::io::Read;
use std::result::Result;

use reqwest::{
    blocking::Client,
    header::{self, HeaderValue},
    Error as HttpError,
};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug)]
pub enum PocketError {
    Http(HttpError, Option<String>),
    Io(IoError),
    SerdeJson(serde_json::Error),
    Proto(String, String, Option<String>),
}

struct HttpErrorWithBody {
    http_error: HttpError,
    body: Option<String>,
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
        PocketError::Http(err, None)
    }
}

impl From<HttpErrorWithBody> for PocketError {
    fn from(err: HttpErrorWithBody) -> PocketError {
        PocketError::Http(err.http_error, err.body)
    }
}

impl Error for PocketError {
    fn cause(&self) -> Option<&dyn Error> {
        match self {
            PocketError::Http(e, _) => Some(e),
            PocketError::Io(e) => Some(e),
            PocketError::SerdeJson(e) => Some(e),
            PocketError::Proto(..) => None,
        }
    }
}

impl fmt::Display for PocketError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            PocketError::Http(e, body) => {
                e.fmt(fmt)?;
                if let Some(body) = body {
                    writeln!(fmt)?;
                    body.fmt(fmt)?;
                }
                Ok(())
            }
            PocketError::Io(e) => e.fmt(fmt),
            PocketError::SerdeJson(e) => e.fmt(fmt),
            PocketError::Proto(code, msg, body) => {
                write!(fmt, "{} (code {})", msg, code)?;
                if let Some(body) = body {
                    writeln!(fmt)?;
                    body.fmt(fmt)?;
                }
                Ok(())
            }
        }
    }
}

const X_ACCEPT: &str = "X-Accept";
const X_ERROR: &str = "X-Error";
const X_ERROR_CODE: &str = "X-Error-Code";

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
    url: &'a Url,
    title: Option<&'a str>,
    tags: Option<&'a str>,
    tweet_id: Option<&'a str>,
}

impl Pocket {
    pub fn new(consumer_key: &str, access_token: Option<&str>, client: Client) -> Pocket {
        Pocket {
            consumer_key: consumer_key.to_string(),
            access_token: access_token.map(|v| v.to_string()),
            code: None,
            client,
        }
    }

    #[inline]
    pub fn access_token(&self) -> Option<&str> {
        self.access_token.as_deref()
    }

    fn request<Req: Serialize>(&self, url: &str, request: &Req) -> PocketResult<String> {
        let request = serde_json::to_string(request)?;

        let app_json = "application/json";

        self.client
            .post(url)
            .header(X_ACCEPT, HeaderValue::from_static(app_json))
            .header(header::CONTENT_TYPE, HeaderValue::from_static(app_json))
            .body(request)
            .send()
            .map_err(From::from)
            .and_then(|mut r| {
                let mut body = String::new();
                let body = r
                    .read_to_string(&mut body)
                    .map_err(From::from)
                    .map(|_| body);

                if let Some(code) = r.headers().get(X_ERROR_CODE) {
                    return Err(PocketError::Proto(
                        code.to_str()
                            .expect("X-Error-Code is not well-formed UTF-8")
                            .into(),
                        r.headers()
                            .get(X_ERROR)
                            .map(|v| v.to_str().expect("X-Error is not well-formed UTF-8").into())
                            .unwrap_or("unknown protocol error".into()),
                        body.ok(),
                    ));
                }

                if !r.status().is_success() {
                    r.error_for_status()
                        .map_err(|http_error| HttpErrorWithBody {
                            http_error,
                            body: body.as_ref().ok().cloned(),
                        })?;
                }

                body
            })
    }

    pub fn get_auth_url(&mut self) -> PocketResult<Url> {
        // The final period is encoded as %2E
        // because on some terminals (e.g. Konsole),
        // the period is excluded from the URL
        // when you Ctrl+click it.
        const REDIRECT_URI: &str =
            "data:text/plain,Return%20to%20feeds-to-pocket%20and%20press%20Enter%20to%20finish%2E";

        let response = {
            // scope to release borrow on self
            let request = PocketOAuthRequest {
                consumer_key: &self.consumer_key,
                redirect_uri: REDIRECT_URI,
                state: None,
            };

            self.request("https://getpocket.com/v3/oauth/request", &request)
        };

        response
            .and_then(|r| r.decode())
            .map(|r: PocketOAuthResponse| {
                let mut url = Url::parse("https://getpocket.com/auth/authorize").unwrap();
                url.query_pairs_mut()
                    .append_pair("request_token", &r.code)
                    .append_pair("redirect_uri", REDIRECT_URI);
                self.code = Some(r.code);
                url
            })
    }

    pub fn authorize(&mut self) -> PocketResult<String> {
        {
            let request = PocketAuthorizeRequest {
                consumer_key: &self.consumer_key,
                code: self.code.as_deref().unwrap(),
            };

            self.request("https://getpocket.com/v3/oauth/authorize", &request)
        }
        .and_then(|r| r.decode())
        .map(|r: PocketAuthorizeResponse| {
            self.access_token = Some(r.access_token);
            r.username
        })
    }

    pub fn add(
        &mut self,
        url: &Url,
        title: Option<&str>,
        tags: Option<&str>,
        tweet_id: Option<&str>,
    ) -> PocketResult<()> {
        let request = PocketAddRequest {
            consumer_key: &self.consumer_key,
            access_token: self.access_token.as_ref().unwrap(),
            url,
            title,
            tags,
            tweet_id,
        };

        self.request("https://getpocket.com/v3/add", &request)
            .map(|_| ())
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
