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

//! Wire-level tests for the GENERATED prost messages under `src/api`.
//!
//! These are the cases that catch a broken generator: a dropped field, a shifted tag number, a
//! presence field silently coerced to its zero value, an enum whose discriminants moved, a oneof
//! that lost a variant. They are pure encode/decode - no runtime, no socket. The gRPC plumbing is
//! covered by `tests/generated_grpc.rs`.
//!
//! The NLU client's cross-package case has no counterpart here: the T2S API compiles to exactly
//! one package, `ondewo.t2s` - it vendors no `google.*` proto and no second ONDEWO package.

use ondewo_t2s_client::api::ondewo::t2s;
use prost::Message;

/// A fully populated [`t2s::RequestConfig`] - scalar, oneof, message and presence fields at once.
///
/// `instruction` is the API's single scalar proto3 `optional` field, which is why every presence
/// case below goes through this message.
fn sample_config() -> t2s::RequestConfig {
    t2s::RequestConfig {
        t2s_pipeline_id: "de_DE_ondewo_vits".to_string(),
        instruction: Some("speak slowly".to_string()),
        oneof_length_scale: Some(t2s::request_config::OneofLengthScale::LengthScale(1.25)),
        oneof_pcm: Some(t2s::request_config::OneofPcm::Pcm(t2s::Pcm::Pcm24 as i32)),
        oneof_audio_format: Some(t2s::request_config::OneofAudioFormat::AudioFormat(
            t2s::AudioFormat::Flac as i32,
        )),
        oneof_use_cache: Some(t2s::request_config::OneofUseCache::UseCache(true)),
        ..Default::default()
    }
}

#[test]
fn a_request_config_survives_a_serialize_parse_round_trip() {
    let original = sample_config();

    let bytes = original.encode_to_vec();
    assert!(
        !bytes.is_empty(),
        "a populated RequestConfig must not encode to zero bytes"
    );
    assert_eq!(
        bytes.len(),
        original.encoded_len(),
        "encoded_len must agree with the bytes actually written"
    );

    let parsed =
        t2s::RequestConfig::decode(bytes.as_slice()).expect("re-parsing our own bytes must work");
    assert_eq!(parsed, original);

    // Spot-check the individual fields too: a PartialEq on two identically broken values would
    // still pass above.
    assert_eq!(parsed.t2s_pipeline_id, "de_DE_ondewo_vits");
    assert_eq!(parsed.instruction.as_deref(), Some("speak slowly"));
    assert_eq!(
        parsed.oneof_length_scale,
        Some(t2s::request_config::OneofLengthScale::LengthScale(1.25))
    );
    assert_eq!(
        parsed.oneof_audio_format,
        Some(t2s::request_config::OneofAudioFormat::AudioFormat(
            t2s::AudioFormat::Flac as i32
        ))
    );
}

#[test]
fn a_default_request_config_round_trips_to_zero_bytes() {
    let empty = t2s::RequestConfig::default();

    assert_eq!(empty.t2s_pipeline_id, "");
    assert_eq!(empty.instruction, None);
    assert_eq!(empty.oneof_length_scale, None);
    assert_eq!(empty.oneof_pcm, None);

    let bytes = empty.encode_to_vec();
    assert!(
        bytes.is_empty(),
        "proto3 must not put unset fields on the wire, got {bytes:?}"
    );
    assert_eq!(t2s::RequestConfig::decode(bytes.as_slice()).unwrap(), empty);
}

/// `RequestConfig.instruction` is the T2S API's only scalar proto3 `optional` (explicit presence)
/// field. An unset field and a field explicitly set to `""` are two DIFFERENT values and must stay
/// distinguishable across the wire - a generator that collapses them makes `""` unsendable.
#[test]
fn an_explicit_presence_field_distinguishes_unset_from_zero() {
    let unset = t2s::RequestConfig {
        t2s_pipeline_id: "p".to_string(),
        instruction: None,
        ..Default::default()
    };
    let explicit_zero = t2s::RequestConfig {
        t2s_pipeline_id: "p".to_string(),
        instruction: Some(String::new()),
        ..Default::default()
    };

    let unset_bytes = unset.encode_to_vec();
    let zero_bytes = explicit_zero.encode_to_vec();
    assert_ne!(
        unset_bytes, zero_bytes,
        "an explicitly set \"\" must occupy the wire, an unset field must not"
    );

    assert_eq!(
        t2s::RequestConfig::decode(unset_bytes.as_slice())
            .unwrap()
            .instruction,
        None
    );
    assert_eq!(
        t2s::RequestConfig::decode(zero_bytes.as_slice())
            .unwrap()
            .instruction,
        Some(String::new())
    );
}

/// A oneof member explicitly set to its zero value is likewise on the wire: `use_cache = false`
/// has to stay distinguishable from "no cache preference expressed".
#[test]
fn a_oneof_set_to_its_zero_value_stays_distinguishable_from_unset() {
    let unset = t2s::RequestConfig::default();
    let explicit_false = t2s::RequestConfig {
        oneof_use_cache: Some(t2s::request_config::OneofUseCache::UseCache(false)),
        ..Default::default()
    };

    assert_ne!(unset.encode_to_vec(), explicit_false.encode_to_vec());
    assert_eq!(
        t2s::RequestConfig::decode(explicit_false.encode_to_vec().as_slice())
            .unwrap()
            .oneof_use_cache,
        Some(t2s::request_config::OneofUseCache::UseCache(false))
    );
}

/// Decoding tolerates fields it does not know: an unknown tag is skipped, not an error.
#[test]
fn decoding_skips_an_unknown_field() {
    let mut bytes = t2s::T2sPipelineId {
        id: "de_DE_ondewo_vits".to_string(),
    }
    .encode_to_vec();
    // tag 999, wire type 0 (varint), value 1
    bytes.extend_from_slice(&[0xB8, 0x3E, 0x01]);

    let parsed = t2s::T2sPipelineId::decode(bytes.as_slice())
        .expect("an unknown field must be skipped, not rejected");
    assert_eq!(parsed.id, "de_DE_ondewo_vits");
}

#[test]
fn decoding_rejects_a_truncated_message() {
    let bytes = t2s::NormalizeTextRequest {
        t2s_pipeline_id: "de_DE_ondewo_vits".to_string(),
        text: "Es ist 12:45 Uhr.".to_string(),
    }
    .encode_to_vec();
    let truncated = &bytes[..bytes.len() - 1];

    assert!(
        t2s::NormalizeTextRequest::decode(truncated).is_err(),
        "a truncated message must not decode silently"
    );
}

/// The zero value of an enum is the one a default-constructed message carries, so it must be the
/// variant the proto declares as `= 0`. The T2S enums name their zero state (`PCM_16`, `wav`,
/// `extend_hard`) rather than an `…_UNSPECIFIED` variant.
#[test]
fn the_enum_zero_value_is_the_variant_the_proto_declares_as_zero() {
    assert_eq!(t2s::Pcm::Pcm16 as i32, 0);
    assert_eq!(t2s::Pcm::try_from(0), Ok(t2s::Pcm::Pcm16));
    assert_eq!(t2s::Pcm::Pcm16.as_str_name(), "PCM_16");
    assert_eq!(t2s::Pcm::from_str_name("PCM_16"), Some(t2s::Pcm::Pcm16));
    assert_eq!(t2s::Pcm::from_str_name("NOT_A_VARIANT"), None);
    assert!(
        t2s::Pcm::try_from(9_999).is_err(),
        "an out-of-range discriminant must not map to a variant"
    );

    // The T2S protos spell the AudioFormat values in lower case, and prost is documented not to
    // transform them - so the name really is "wav", not "WAV".
    assert_eq!(t2s::AudioFormat::Wav as i32, 0);
    assert_eq!(t2s::AudioFormat::Wav.as_str_name(), "wav");
    assert_eq!(
        t2s::AudioFormat::from_str_name("wav"),
        Some(t2s::AudioFormat::Wav)
    );

    use t2s::update_custom_phonemizer_request::UpdateMethod;
    assert_eq!(UpdateMethod::ExtendHard as i32, 0);
    assert_eq!(UpdateMethod::ExtendHard.as_str_name(), "extend_hard");
    assert_eq!(
        t2s::UpdateCustomPhonemizerRequest::default().update_method,
        UpdateMethod::ExtendHard as i32,
        "a default message must carry the enum's zero value"
    );
}

/// A non-zero enum value has to travel as its discriminant, not as the zero value.
#[test]
fn a_non_zero_enum_value_round_trips() {
    use t2s::update_custom_phonemizer_request::UpdateMethod;

    let request = t2s::UpdateCustomPhonemizerRequest {
        id: "de_DE_ondewo_vits".to_string(),
        update_method: UpdateMethod::Replace as i32,
        maps: vec![t2s::Map {
            word: "ONDEWO".to_string(),
            phoneme_groups: "ˈɔndeˌvoː".to_string(),
        }],
    };

    let parsed =
        t2s::UpdateCustomPhonemizerRequest::decode(request.encode_to_vec().as_slice()).unwrap();
    assert_eq!(parsed, request);
    assert_eq!(
        UpdateMethod::try_from(parsed.update_method),
        Ok(UpdateMethod::Replace)
    );
}

/// Repeated and nested message fields have to nest, not flatten.
#[test]
fn a_nested_and_repeated_message_round_trips() {
    let response = t2s::BatchSynthesizeResponse {
        batch_response: vec![
            t2s::SynthesizeResponse {
                audio_uuid: "audio-1".to_string(),
                audio: vec![0x52, 0x49, 0x46, 0x46],
                generation_time: 0.25,
                audio_length: 1.5,
                text: "Guten Tag".to_string(),
                config: Some(sample_config()),
                normalized_text: "Guten Tag".to_string(),
                sample_rate: 22_050.0,
            },
            t2s::SynthesizeResponse {
                audio_uuid: "audio-2".to_string(),
                ..Default::default()
            },
        ],
    };

    let parsed = t2s::BatchSynthesizeResponse::decode(response.encode_to_vec().as_slice()).unwrap();
    assert_eq!(parsed, response);
    assert_eq!(parsed.batch_response.len(), 2);
    assert_eq!(parsed.batch_response[1].audio_uuid, "audio-2");
    assert_eq!(
        parsed.batch_response[0]
            .config
            .as_ref()
            .unwrap()
            .instruction
            .as_deref(),
        Some("speak slowly"),
        "the nested presence field must survive being nested"
    );
    assert_eq!(parsed.batch_response[1].config, None);
}

/// A repeated message field keeps its element order and its element boundaries.
#[test]
fn a_repeated_message_field_keeps_its_order() {
    let phonemizer = t2s::CustomPhonemizerProto {
        id: "de_DE_ondewo_vits".to_string(),
        maps: vec![
            t2s::Map {
                word: "ONDEWO".to_string(),
                phoneme_groups: "ˈɔndeˌvoː".to_string(),
            },
            t2s::Map {
                word: "gRPC".to_string(),
                phoneme_groups: "d͡ʒiːɑːɹpiːsiː".to_string(),
            },
        ],
    };

    let parsed = t2s::CustomPhonemizerProto::decode(phonemizer.encode_to_vec().as_slice()).unwrap();
    assert_eq!(parsed, phonemizer);
    assert_eq!(parsed.maps[0].word, "ONDEWO");
    assert_eq!(parsed.maps[1].word, "gRPC");
}
