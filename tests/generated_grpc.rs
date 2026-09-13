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

//! End-to-end tests for the GENERATED tonic service stubs.
//!
//! The generated `Text2SpeechServer` is served over a loopback socket and driven by the generated
//! `Text2SpeechClient`, so a request really is encoded, routed by its
//! `/ondewo.t2s.Text2Speech/<Method>` path, decoded, answered and decoded again. That is what
//! catches a service the generator wired to the wrong path, a codec mismatch, or a method that
//! silently went missing.
//!
//! `Text2Speech` carries one streaming RPC, `StreamingSynthesize` (client stream in, server stream
//! out), so the fake server also has to declare the generated `StreamingSynthesizeStream`
//! associated type; the round-trip, error and metadata assertions stay on unary RPCs.
//!
//! No network beyond `127.0.0.1` and no ONDEWO server is involved.

use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ondewo_t2s_client::api::ondewo::t2s;
use ondewo_t2s_client::api::ondewo::t2s::text2_speech_client::Text2SpeechClient;
use ondewo_t2s_client::api::ondewo::t2s::text2_speech_server::{Text2Speech, Text2SpeechServer};
use ondewo_t2s_client::auth::{
    BearerTokenInterceptor, AUTHORIZATION_METADATA_KEY, CAI_TOKEN_METADATA_KEY,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::Stream;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Code, Request, Response, Status};

/// The pipeline id `get_t2s_pipeline` answers with `not_found` for, so the error path is exercised.
const MISSING_PIPELINE: &str = "does-not-exist";

/// Metadata the fake server captured from the last request it handled.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SeenMetadata {
    authorization: Option<String>,
    cai_token: Option<String>,
}

/// A minimal in-process implementation of the generated `Text2Speech` service.
#[derive(Clone, Default)]
struct FakeText2Speech {
    seen: Arc<Mutex<SeenMetadata>>,
}

impl FakeText2Speech {
    fn record<T>(&self, request: &Request<T>) {
        let read = |key: &str| {
            request
                .metadata()
                .get(key)
                .map(|value| value.to_str().unwrap().to_string())
        };
        *self.seen.lock().unwrap() = SeenMetadata {
            authorization: read(AUTHORIZATION_METADATA_KEY),
            cai_token: read(CAI_TOKEN_METADATA_KEY),
        };
    }

    fn seen(&self) -> SeenMetadata {
        self.seen.lock().unwrap().clone()
    }
}

/// Synthesize `text` under `config`, echoing both back the way the real service does.
fn synthesized(text: String, config: Option<t2s::RequestConfig>) -> t2s::SynthesizeResponse {
    t2s::SynthesizeResponse {
        audio_uuid: format!("audio-{}", text.len()),
        audio: text.as_bytes().to_vec(),
        generation_time: 0.25,
        audio_length: 1.5,
        normalized_text: text.to_lowercase(),
        text,
        config,
        sample_rate: 22_050.0,
    }
}

#[tonic::async_trait]
impl Text2Speech for FakeText2Speech {
    /// Echoes the request config straight back into the response, so the presence field the client
    /// sent has to survive a real gRPC hop in both directions.
    async fn synthesize(
        &self,
        request: Request<t2s::SynthesizeRequest>,
    ) -> Result<Response<t2s::SynthesizeResponse>, Status> {
        self.record(&request);
        let request = request.into_inner();
        Ok(Response::new(synthesized(request.text, request.config)))
    }

    async fn batch_synthesize(
        &self,
        request: Request<t2s::BatchSynthesizeRequest>,
    ) -> Result<Response<t2s::BatchSynthesizeResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::BatchSynthesizeResponse {
            batch_response: request
                .into_inner()
                .batch_request
                .into_iter()
                .map(|single| synthesized(single.text, single.config))
                .collect(),
        }))
    }

    type StreamingSynthesizeStream =
        Pin<Box<dyn Stream<Item = Result<t2s::StreamingSynthesizeResponse, Status>> + Send>>;

    /// Drains the client stream and answers with one chunk per request, so both halves of the
    /// bidirectional RPC are really carried over the socket.
    async fn streaming_synthesize(
        &self,
        request: Request<tonic::Streaming<t2s::StreamingSynthesizeRequest>>,
    ) -> Result<Response<Self::StreamingSynthesizeStream>, Status> {
        self.record(&request);
        let mut incoming = request.into_inner();
        let mut chunks = Vec::new();
        while let Some(chunk) = incoming.message().await? {
            let synthesized = synthesized(chunk.text, chunk.config);
            chunks.push(Ok(t2s::StreamingSynthesizeResponse {
                audio_uuid: synthesized.audio_uuid,
                audio: synthesized.audio,
                generation_time: synthesized.generation_time,
                audio_length: synthesized.audio_length,
                text: synthesized.text,
                config: synthesized.config,
                normalized_text: synthesized.normalized_text,
                sample_rate: synthesized.sample_rate,
            }));
        }
        Ok(Response::new(Box::pin(tokio_stream::iter(chunks))))
    }

    async fn normalize_text(
        &self,
        request: Request<t2s::NormalizeTextRequest>,
    ) -> Result<Response<t2s::NormalizeTextResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::NormalizeTextResponse {
            normalized_text: request.into_inner().text.to_lowercase(),
        }))
    }

    async fn get_t2s_pipeline(
        &self,
        request: Request<t2s::T2sPipelineId>,
    ) -> Result<Response<t2s::Text2SpeechConfig>, Status> {
        self.record(&request);
        let id = request.into_inner().id;
        if id == MISSING_PIPELINE {
            return Err(Status::not_found(format!("no t2s pipeline named {id}")));
        }
        Ok(Response::new(t2s::Text2SpeechConfig {
            id,
            active: true,
            ..Default::default()
        }))
    }

    async fn create_t2s_pipeline(
        &self,
        request: Request<t2s::Text2SpeechConfig>,
    ) -> Result<Response<t2s::T2sPipelineId>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::T2sPipelineId {
            id: request.into_inner().id,
        }))
    }

    async fn delete_t2s_pipeline(
        &self,
        request: Request<t2s::T2sPipelineId>,
    ) -> Result<Response<()>, Status> {
        self.record(&request);
        Ok(Response::new(()))
    }

    async fn update_t2s_pipeline(
        &self,
        request: Request<t2s::Text2SpeechConfig>,
    ) -> Result<Response<()>, Status> {
        self.record(&request);
        Ok(Response::new(()))
    }

    async fn list_t2s_pipelines(
        &self,
        request: Request<t2s::ListT2sPipelinesRequest>,
    ) -> Result<Response<t2s::ListT2sPipelinesResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::ListT2sPipelinesResponse {
            pipelines: request
                .into_inner()
                .languages
                .into_iter()
                .map(|language| t2s::Text2SpeechConfig {
                    id: format!("{language}_ondewo_vits"),
                    description: Some(t2s::T2sDescription {
                        language,
                        ..Default::default()
                    }),
                    active: true,
                    ..Default::default()
                })
                .collect(),
        }))
    }

    async fn list_t2s_languages(
        &self,
        request: Request<t2s::ListT2sLanguagesRequest>,
    ) -> Result<Response<t2s::ListT2sLanguagesResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::ListT2sLanguagesResponse {
            languages: vec!["de".to_string(), "en".to_string()],
        }))
    }

    async fn list_t2s_domains(
        &self,
        request: Request<t2s::ListT2sDomainsRequest>,
    ) -> Result<Response<t2s::ListT2sDomainsResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::ListT2sDomainsResponse {
            domains: vec!["general".to_string()],
        }))
    }

    async fn list_t2s_normalization_pipelines(
        &self,
        request: Request<t2s::ListT2sNormalizationPipelinesRequest>,
    ) -> Result<Response<t2s::ListT2sNormalizationPipelinesResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::ListT2sNormalizationPipelinesResponse {
            t2s_normalization_pipelines: vec![request.into_inner().language],
        }))
    }

    async fn get_service_info(
        &self,
        request: Request<()>,
    ) -> Result<Response<t2s::T2sGetServiceInfoResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::T2sGetServiceInfoResponse {
            version: "6.6.0".to_string(),
        }))
    }

    async fn get_custom_phonemizer(
        &self,
        request: Request<t2s::PhonemizerId>,
    ) -> Result<Response<t2s::CustomPhonemizerProto>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::CustomPhonemizerProto {
            id: request.into_inner().id,
            maps: vec![t2s::Map {
                word: "ONDEWO".to_string(),
                phoneme_groups: "ˈɔndeˌvoː".to_string(),
            }],
        }))
    }

    async fn create_custom_phonemizer(
        &self,
        request: Request<t2s::CreateCustomPhonemizerRequest>,
    ) -> Result<Response<t2s::PhonemizerId>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::PhonemizerId {
            id: request.into_inner().prefix,
        }))
    }

    async fn delete_custom_phonemizer(
        &self,
        request: Request<t2s::PhonemizerId>,
    ) -> Result<Response<()>, Status> {
        self.record(&request);
        Ok(Response::new(()))
    }

    /// Echoes the submitted maps back, so the repeated nested message really makes the round trip.
    async fn update_custom_phonemizer(
        &self,
        request: Request<t2s::UpdateCustomPhonemizerRequest>,
    ) -> Result<Response<t2s::CustomPhonemizerProto>, Status> {
        self.record(&request);
        let request = request.into_inner();
        Ok(Response::new(t2s::CustomPhonemizerProto {
            id: request.id,
            maps: request.maps,
        }))
    }

    async fn list_custom_phonemizer(
        &self,
        request: Request<t2s::ListCustomPhonemizerRequest>,
    ) -> Result<Response<t2s::ListCustomPhonemizerResponse>, Status> {
        self.record(&request);
        Ok(Response::new(t2s::ListCustomPhonemizerResponse {
            phonemizers: request
                .into_inner()
                .pipeline_ids
                .into_iter()
                .map(|id| t2s::CustomPhonemizerProto {
                    id,
                    maps: Vec::new(),
                })
                .collect(),
        }))
    }

    async fn voice_cloning(
        &self,
        request: Request<t2s::VoiceCloningRequest>,
    ) -> Result<Response<()>, Status> {
        self.record(&request);
        Ok(Response::new(()))
    }
}

/// Start the generated server on an ephemeral loopback port and return it with its address.
///
/// The server task is detached; it ends when the test process does.
async fn start_server() -> (FakeText2Speech, SocketAddr) {
    let service = FakeText2Speech::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    let served = service.clone();
    tokio::spawn(async move {
        Server::builder()
            .add_service(Text2SpeechServer::new(served))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .expect("the in-process gRPC server must not fail");
    });

    (service, addr)
}

async fn connect(addr: SocketAddr) -> Channel {
    Endpoint::from_shared(format!("http://{addr}"))
        .expect("endpoint")
        .connect_timeout(Duration::from_secs(10))
        .connect()
        .await
        .expect("the in-process gRPC server must accept a connection")
}

fn config(instruction: Option<&str>) -> t2s::RequestConfig {
    t2s::RequestConfig {
        t2s_pipeline_id: "de_DE_ondewo_vits".to_string(),
        instruction: instruction.map(str::to_string),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_unary_call_round_trips_through_the_generated_client_and_server() {
    let (_service, addr) = start_server().await;
    let mut client = Text2SpeechClient::new(connect(addr).await);

    let response = client
        .synthesize(t2s::SynthesizeRequest {
            text: "Guten Tag".to_string(),
            config: Some(config(Some("speak slowly"))),
        })
        .await
        .expect("Synthesize must succeed")
        .into_inner();

    assert_eq!(response.text, "Guten Tag");
    assert_eq!(response.normalized_text, "guten tag");
    assert_eq!(response.audio, b"Guten Tag");
    assert_eq!(response.sample_rate, 22_050.0);
    assert_eq!(
        response.config.unwrap().instruction.as_deref(),
        Some("speak slowly")
    );
}

/// The explicit-presence guarantee of `tests/generated_messages.rs`, but over a real hop: the
/// client sends `Some("")` in and gets `Some("")` back, never `None`.
#[tokio::test]
async fn an_explicit_presence_field_survives_a_real_grpc_hop() {
    let (_service, addr) = start_server().await;
    let mut client = Text2SpeechClient::new(connect(addr).await);

    let echoed = client
        .synthesize(t2s::SynthesizeRequest {
            text: "Guten Tag".to_string(),
            config: Some(config(Some(""))),
        })
        .await
        .expect("Synthesize must succeed")
        .into_inner();

    assert_eq!(
        echoed.config.unwrap().instruction,
        Some(String::new()),
        "an explicitly empty presence field must not come back unset"
    );

    let unset = client
        .synthesize(t2s::SynthesizeRequest {
            text: "Guten Tag".to_string(),
            config: Some(config(None)),
        })
        .await
        .expect("Synthesize must succeed")
        .into_inner();

    assert_eq!(
        unset.config.unwrap().instruction,
        None,
        "an unset presence field must not come back as its zero value"
    );
}

/// The one streaming RPC of the service: the client streams requests in and reads a response
/// stream back, so both directions of `StreamingSynthesize` are really carried over the socket.
#[tokio::test]
async fn the_streaming_rpc_carries_every_chunk_in_both_directions() {
    let (_service, addr) = start_server().await;
    let mut client = Text2SpeechClient::new(connect(addr).await);

    let requests = vec![
        t2s::StreamingSynthesizeRequest {
            text: "Guten".to_string(),
            config: Some(config(Some("speak slowly"))),
        },
        t2s::StreamingSynthesizeRequest {
            text: "Tag".to_string(),
            config: Some(config(None)),
        },
    ];

    let mut responses = client
        .streaming_synthesize(tokio_stream::iter(requests))
        .await
        .expect("StreamingSynthesize must succeed")
        .into_inner();

    let mut texts = Vec::new();
    while let Some(chunk) = responses
        .message()
        .await
        .expect("the response stream must not fail")
    {
        texts.push(chunk.text);
    }

    assert_eq!(texts, vec!["Guten".to_string(), "Tag".to_string()]);
}

/// A server-side `Status` has to reach the caller as that same status, not as a transport error.
#[tokio::test]
async fn a_server_error_reaches_the_client_as_its_status() {
    let (_service, addr) = start_server().await;
    let mut client = Text2SpeechClient::new(connect(addr).await);

    let error = client
        .get_t2s_pipeline(t2s::T2sPipelineId {
            id: MISSING_PIPELINE.to_string(),
        })
        .await
        .expect_err("GetT2sPipeline must report the missing pipeline");

    assert_eq!(error.code(), Code::NotFound);
    assert_eq!(error.message(), "no t2s pipeline named does-not-exist");
}

/// Every RPC the `Text2Speech` proto declares must exist on the generated client and be routable -
/// a method the generator dropped, or wired to the wrong path, fails here with `Unimplemented`.
///
/// `GetServiceInfo` takes `google.protobuf.Empty`, which prost models as `()`.
#[tokio::test]
async fn every_declared_service_method_exists_and_is_routable() {
    let (_service, addr) = start_server().await;
    let mut client = Text2SpeechClient::new(connect(addr).await);

    client
        .synthesize(t2s::SynthesizeRequest::default())
        .await
        .expect("Synthesize");
    client
        .batch_synthesize(t2s::BatchSynthesizeRequest::default())
        .await
        .expect("BatchSynthesize");
    client
        .streaming_synthesize(tokio_stream::iter(vec![
            t2s::StreamingSynthesizeRequest::default(),
        ]))
        .await
        .expect("StreamingSynthesize");
    client
        .normalize_text(t2s::NormalizeTextRequest::default())
        .await
        .expect("NormalizeText");
    client
        .get_t2s_pipeline(t2s::T2sPipelineId {
            id: "de_DE_ondewo_vits".to_string(),
        })
        .await
        .expect("GetT2sPipeline");
    client
        .create_t2s_pipeline(t2s::Text2SpeechConfig::default())
        .await
        .expect("CreateT2sPipeline");
    client
        .delete_t2s_pipeline(t2s::T2sPipelineId::default())
        .await
        .expect("DeleteT2sPipeline");
    client
        .update_t2s_pipeline(t2s::Text2SpeechConfig::default())
        .await
        .expect("UpdateT2sPipeline");
    client
        .list_t2s_pipelines(t2s::ListT2sPipelinesRequest::default())
        .await
        .expect("ListT2sPipelines");
    client
        .list_t2s_languages(t2s::ListT2sLanguagesRequest::default())
        .await
        .expect("ListT2sLanguages");
    client
        .list_t2s_domains(t2s::ListT2sDomainsRequest::default())
        .await
        .expect("ListT2sDomains");
    client
        .list_t2s_normalization_pipelines(t2s::ListT2sNormalizationPipelinesRequest::default())
        .await
        .expect("ListT2sNormalizationPipelines");
    client.get_service_info(()).await.expect("GetServiceInfo");
    client
        .get_custom_phonemizer(t2s::PhonemizerId::default())
        .await
        .expect("GetCustomPhonemizer");
    client
        .create_custom_phonemizer(t2s::CreateCustomPhonemizerRequest::default())
        .await
        .expect("CreateCustomPhonemizer");
    client
        .delete_custom_phonemizer(t2s::PhonemizerId::default())
        .await
        .expect("DeleteCustomPhonemizer");
    client
        .update_custom_phonemizer(t2s::UpdateCustomPhonemizerRequest::default())
        .await
        .expect("UpdateCustomPhonemizer");
    client
        .list_custom_phonemizer(t2s::ListCustomPhonemizerRequest::default())
        .await
        .expect("ListCustomPhonemizer");
    client
        .voice_cloning(t2s::VoiceCloningRequest::default())
        .await
        .expect("VoiceCloning");
}

/// A repeated nested message has to survive the hop with its order and its elements intact.
#[tokio::test]
async fn a_repeated_nested_message_survives_a_real_grpc_hop() {
    let (_service, addr) = start_server().await;
    let mut client = Text2SpeechClient::new(connect(addr).await);

    let maps = vec![
        t2s::Map {
            word: "ONDEWO".to_string(),
            phoneme_groups: "ˈɔndeˌvoː".to_string(),
        },
        t2s::Map {
            word: "gRPC".to_string(),
            phoneme_groups: "d͡ʒiːɑːɹpiːsiː".to_string(),
        },
    ];

    let echoed = client
        .update_custom_phonemizer(t2s::UpdateCustomPhonemizerRequest {
            id: "de_DE_ondewo_vits".to_string(),
            update_method: t2s::update_custom_phonemizer_request::UpdateMethod::Replace as i32,
            maps: maps.clone(),
        })
        .await
        .expect("UpdateCustomPhonemizer must succeed")
        .into_inner();

    assert_eq!(echoed.id, "de_DE_ondewo_vits");
    assert_eq!(echoed.maps, maps);
}

/// The hand-written [`BearerTokenInterceptor`] has to put its metadata on the wire, where the
/// server can actually read it - asserting on the `Request` it returns would not prove that.
#[tokio::test]
async fn the_bearer_interceptor_reaches_the_server() {
    let (service, addr) = start_server().await;
    let interceptor = BearerTokenInterceptor::new("access-token-abc")
        .expect("a plain ASCII token is valid")
        .with_cai_token("cai-token-xyz")
        .expect("a plain ASCII cai token is valid");
    let mut client = Text2SpeechClient::with_interceptor(connect(addr).await, interceptor);

    client.get_service_info(()).await.expect("GetServiceInfo");

    assert_eq!(
        service.seen(),
        SeenMetadata {
            authorization: Some("Bearer access-token-abc".to_string()),
            cai_token: Some("cai-token-xyz".to_string()),
        }
    );
}

/// Without the interceptor the client must send no credentials at all - the unauthenticated path
/// (plaintext server, or an ingress that injects the bearer token) has to stay usable.
#[tokio::test]
async fn a_client_without_an_interceptor_sends_no_credentials() {
    let (service, addr) = start_server().await;
    let mut client = Text2SpeechClient::new(connect(addr).await);

    client.get_service_info(()).await.expect("GetServiceInfo");

    assert_eq!(service.seen(), SeenMetadata::default());
}

/// A client built against an address nothing listens on must surface a transport error rather
/// than panic or hang - `connect_lazy` defers the connect to the first call.
#[tokio::test]
async fn a_call_to_an_unreachable_target_fails_as_a_status() {
    let channel = Endpoint::from_static("http://127.0.0.1:1")
        .connect_timeout(Duration::from_secs(2))
        .connect_lazy();
    let mut client = Text2SpeechClient::new(channel);

    let error = client
        .get_service_info(())
        .await
        .expect_err("nothing listens on port 1");

    assert_eq!(error.code(), Code::Unavailable);
}
