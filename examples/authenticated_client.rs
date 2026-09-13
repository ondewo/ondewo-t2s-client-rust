// Copyright 2021-2026 ONDEWO GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Synthesize speech on an ONDEWO T2S server with a Keycloak bearer token.
//!
//! This is the crate's usage snippet. It lives here rather than in a doc comment because doctests
//! are disabled crate-wide (see the `doctest = false` note in `Cargo.toml`); as an example it is
//! still compiled by `cargo test` and `cargo build --examples`, so it cannot rot.
//!
//! ```sh
//! ONDEWO_T2S_HOST=https://t2s.example.com:443 \
//! ONDEWO_T2S_ACCESS_TOKEN=<keycloak access token> \
//! ONDEWO_T2S_CAI_TOKEN=<cai token> \
//! ONDEWO_T2S_PIPELINE_ID=de_DE_ondewo_vits \
//!   cargo run --example authenticated_client
//! ```

use std::env;
use std::error::Error;

use ondewo_t2s_client::api::ondewo::t2s;
use ondewo_t2s_client::api::ondewo::t2s::text2_speech_client::Text2SpeechClient;
use ondewo_t2s_client::auth::BearerTokenInterceptor;
use tonic::transport::Endpoint;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let host = env::var("ONDEWO_T2S_HOST")?;
    let access_token = env::var("ONDEWO_T2S_ACCESS_TOKEN")?;

    let mut interceptor = BearerTokenInterceptor::new(&access_token)?;
    if let Ok(cai_token) = env::var("ONDEWO_T2S_CAI_TOKEN") {
        interceptor = interceptor.with_cai_token(&cai_token)?;
    }

    let channel = Endpoint::from_shared(host)?.connect().await?;
    let mut client = Text2SpeechClient::with_interceptor(channel, interceptor);

    let response = client
        .synthesize(t2s::SynthesizeRequest {
            text: env::var("ONDEWO_T2S_TEXT").unwrap_or_else(|_| "Guten Tag".to_string()),
            config: Some(t2s::RequestConfig {
                t2s_pipeline_id: env::var("ONDEWO_T2S_PIPELINE_ID")?,
                ..Default::default()
            }),
        })
        .await?
        .into_inner();

    println!(
        "{}: {} bytes of audio at {} Hz ({} s)",
        response.audio_uuid,
        response.audio.len(),
        response.sample_rate,
        response.audio_length
    );
    Ok(())
}
