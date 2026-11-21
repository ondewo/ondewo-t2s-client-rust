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

//! Call an ONDEWO T2S server with an auto-refreshed Keycloak token.
//!
//! With `ONDEWO_T2S_OFFLINE_TOKEN` set, the client starts up from that offline refresh token and
//! never sends a password; otherwise it logs in with `ONDEWO_T2S_PASSWORD`.
//!
//! ```sh
//! ONDEWO_T2S_HOST=t2s.example.com ONDEWO_T2S_PORT=443 \
//! ONDEWO_KEYCLOAK_URL=https://keycloak.example.com/auth \
//! ONDEWO_KEYCLOAK_REALM=ondewo-ccai-platform \
//! ONDEWO_KEYCLOAK_CLIENT_ID=ondewo-t2s-cai-sdk-public \
//! ONDEWO_T2S_USERNAME=technical-user@example.com \
//! ONDEWO_T2S_OFFLINE_TOKEN=<offline refresh token> \
//! ONDEWO_T2S_PIPELINE_ID=de_DE_ondewo_vits \
//!   cargo run --example keycloak_client
//! ```

use std::env;
use std::error::Error;

use ondewo_t2s_client::api::ondewo::t2s;
use ondewo_t2s_client::api::ondewo::t2s::text2_speech_client::Text2SpeechClient;
use ondewo_t2s_client::channel::ClientConfig;
use ondewo_t2s_client::keycloak::{KeycloakConfig, KeycloakInterceptor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = KeycloakConfig::new(
        env::var("ONDEWO_KEYCLOAK_URL")?,
        env::var("ONDEWO_KEYCLOAK_REALM")?,
        env::var("ONDEWO_KEYCLOAK_CLIENT_ID")?,
        env::var("ONDEWO_T2S_USERNAME").unwrap_or_default(),
        env::var("ONDEWO_T2S_PASSWORD").unwrap_or_default(),
    )
    .with_refresh_token(env::var("ONDEWO_T2S_OFFLINE_TOKEN").unwrap_or_default());
    // Validates the config, then acquires the first token; the provider keeps it fresh.
    let interceptor = KeycloakInterceptor::new(&config)?;

    let channel = ClientConfig::new(
        env::var("ONDEWO_T2S_HOST")?,
        env::var("ONDEWO_T2S_PORT")?.parse()?,
    )
    .connect()
    .await?;
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
