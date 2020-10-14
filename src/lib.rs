//! This crate gives an high level api to execute external request.
//!
//! It is supposed to give the basics building blocks for building bridges to the external world
//! while abstracting the low level stuffs like adding custom headers and request tracing.
//!
//! Right now it supports Rest and GraphQL requests.
//!
//! You should start by creating a [Bridge](struct.Bridge.html) instance.
//! This instance should live for all the application lifetime.
//!
//! **Do not create a new bridge on every request!**
//!
//! You should use something like [once_cell](https://crates.io/crates/once_cell) or [lazy_static](https://crates.io/crates/lazy_static), or some sort of inversion of control container to
//! pass around.
//!
//! The bridge implement a type state pattern to build the external request.

mod errors;
pub mod prelude;
mod request;
mod response;

pub use self::{
    request::{Request, RequestType},
    response::Response,
};
use crate::request::Body;
use reqwest::Url;
use serde::Serialize;

/// The bridge instance to issue external requests.
#[derive(Debug)]
pub struct Bridge {
    #[cfg(feature = "blocking")]
    client: reqwest::blocking::Client,
    #[cfg(not(feature = "blocking"))]
    client: reqwest::Client,
    /// the url this bridge should call to
    endpoint: Url,
}

impl Bridge {
    pub fn new(endpoint: Url) -> Self {
        Self {
            #[cfg(feature = "blocking")]
            client: reqwest::blocking::Client::new(),
            #[cfg(not(feature = "blocking"))]
            client: reqwest::Client::new(),
            endpoint,
        }
    }

    pub fn request<S: Serialize, T: Into<Body>>(
        &self,
        request_type: RequestType<S, T>,
    ) -> Request<S, T> {
        Request::new(&self, request_type)
    }
}
