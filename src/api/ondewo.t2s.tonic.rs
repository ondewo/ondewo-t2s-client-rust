// @generated
/// Generated client implementations.
pub mod text2_speech_client {
    #![allow(
        unused_variables,
        dead_code,
        missing_docs,
        clippy::wildcard_imports,
        clippy::let_unit_value,
    )]
    use tonic::codegen::*;
    use tonic::codegen::http::Uri;
    #[derive(Debug, Clone)]
    pub struct Text2SpeechClient<T> {
        inner: tonic::client::Grpc<T>,
    }
    impl Text2SpeechClient<tonic::transport::Channel> {
        /// Attempt to create a new client by connecting to a given endpoint.
        pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>
        where
            D: TryInto<tonic::transport::Endpoint>,
            D::Error: Into<StdError>,
        {
            let conn = tonic::transport::Endpoint::new(dst)?.connect().await?;
            Ok(Self::new(conn))
        }
    }
    impl<T> Text2SpeechClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::Body>,
        T::Error: Into<StdError>,
        T::ResponseBody: Body<Data = Bytes> + std::marker::Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + std::marker::Send,
    {
        pub fn new(inner: T) -> Self {
            let inner = tonic::client::Grpc::new(inner);
            Self { inner }
        }
        pub fn with_origin(inner: T, origin: Uri) -> Self {
            let inner = tonic::client::Grpc::with_origin(inner, origin);
            Self { inner }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> Text2SpeechClient<InterceptedService<T, F>>
        where
            F: tonic::service::Interceptor,
            T::ResponseBody: Default,
            T: tonic::codegen::Service<
                http::Request<tonic::body::Body>,
                Response = http::Response<
                    <T as tonic::client::GrpcService<tonic::body::Body>>::ResponseBody,
                >,
            >,
            <T as tonic::codegen::Service<
                http::Request<tonic::body::Body>,
            >>::Error: Into<StdError> + std::marker::Send + std::marker::Sync,
        {
            Text2SpeechClient::new(InterceptedService::new(inner, interceptor))
        }
        /// Compress requests with the given encoding.
        ///
        /// This requires the server to support it otherwise it might respond with an
        /// error.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.send_compressed(encoding);
            self
        }
        /// Enable decompressing responses.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.inner = self.inner.accept_compressed(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_decoding_message_size(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.inner = self.inner.max_encoding_message_size(limit);
            self
        }
        pub async fn synthesize(
            &mut self,
            request: impl tonic::IntoRequest<super::SynthesizeRequest>,
        ) -> std::result::Result<
            tonic::Response<super::SynthesizeResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/Synthesize",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "Synthesize"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn batch_synthesize(
            &mut self,
            request: impl tonic::IntoRequest<super::BatchSynthesizeRequest>,
        ) -> std::result::Result<
            tonic::Response<super::BatchSynthesizeResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/BatchSynthesize",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "BatchSynthesize"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn streaming_synthesize(
            &mut self,
            request: impl tonic::IntoStreamingRequest<
                Message = super::StreamingSynthesizeRequest,
            >,
        ) -> std::result::Result<
            tonic::Response<tonic::codec::Streaming<super::StreamingSynthesizeResponse>>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/StreamingSynthesize",
            );
            let mut req = request.into_streaming_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("ondewo.t2s.Text2Speech", "StreamingSynthesize"),
                );
            self.inner.streaming(req, path, codec).await
        }
        pub async fn normalize_text(
            &mut self,
            request: impl tonic::IntoRequest<super::NormalizeTextRequest>,
        ) -> std::result::Result<
            tonic::Response<super::NormalizeTextResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/NormalizeText",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "NormalizeText"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn get_t2s_pipeline(
            &mut self,
            request: impl tonic::IntoRequest<super::T2sPipelineId>,
        ) -> std::result::Result<
            tonic::Response<super::Text2SpeechConfig>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/GetT2sPipeline",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "GetT2sPipeline"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn create_t2s_pipeline(
            &mut self,
            request: impl tonic::IntoRequest<super::Text2SpeechConfig>,
        ) -> std::result::Result<tonic::Response<super::T2sPipelineId>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/CreateT2sPipeline",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "CreateT2sPipeline"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn delete_t2s_pipeline(
            &mut self,
            request: impl tonic::IntoRequest<super::T2sPipelineId>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/DeleteT2sPipeline",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "DeleteT2sPipeline"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn update_t2s_pipeline(
            &mut self,
            request: impl tonic::IntoRequest<super::Text2SpeechConfig>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/UpdateT2sPipeline",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "UpdateT2sPipeline"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn list_t2s_pipelines(
            &mut self,
            request: impl tonic::IntoRequest<super::ListT2sPipelinesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sPipelinesResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/ListT2sPipelines",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "ListT2sPipelines"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn list_t2s_languages(
            &mut self,
            request: impl tonic::IntoRequest<super::ListT2sLanguagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sLanguagesResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/ListT2sLanguages",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "ListT2sLanguages"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn list_t2s_domains(
            &mut self,
            request: impl tonic::IntoRequest<super::ListT2sDomainsRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sDomainsResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/ListT2sDomains",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "ListT2sDomains"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn list_t2s_normalization_pipelines(
            &mut self,
            request: impl tonic::IntoRequest<super::ListT2sNormalizationPipelinesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sNormalizationPipelinesResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/ListT2sNormalizationPipelines",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new(
                        "ondewo.t2s.Text2Speech",
                        "ListT2sNormalizationPipelines",
                    ),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn get_service_info(
            &mut self,
            request: impl tonic::IntoRequest<()>,
        ) -> std::result::Result<
            tonic::Response<super::T2sGetServiceInfoResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/GetServiceInfo",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "GetServiceInfo"));
            self.inner.unary(req, path, codec).await
        }
        pub async fn get_custom_phonemizer(
            &mut self,
            request: impl tonic::IntoRequest<super::PhonemizerId>,
        ) -> std::result::Result<
            tonic::Response<super::CustomPhonemizerProto>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/GetCustomPhonemizer",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("ondewo.t2s.Text2Speech", "GetCustomPhonemizer"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn create_custom_phonemizer(
            &mut self,
            request: impl tonic::IntoRequest<super::CreateCustomPhonemizerRequest>,
        ) -> std::result::Result<tonic::Response<super::PhonemizerId>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/CreateCustomPhonemizer",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("ondewo.t2s.Text2Speech", "CreateCustomPhonemizer"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn delete_custom_phonemizer(
            &mut self,
            request: impl tonic::IntoRequest<super::PhonemizerId>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/DeleteCustomPhonemizer",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("ondewo.t2s.Text2Speech", "DeleteCustomPhonemizer"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn update_custom_phonemizer(
            &mut self,
            request: impl tonic::IntoRequest<super::UpdateCustomPhonemizerRequest>,
        ) -> std::result::Result<
            tonic::Response<super::CustomPhonemizerProto>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/UpdateCustomPhonemizer",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("ondewo.t2s.Text2Speech", "UpdateCustomPhonemizer"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn list_custom_phonemizer(
            &mut self,
            request: impl tonic::IntoRequest<super::ListCustomPhonemizerRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListCustomPhonemizerResponse>,
            tonic::Status,
        > {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/ListCustomPhonemizer",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(
                    GrpcMethod::new("ondewo.t2s.Text2Speech", "ListCustomPhonemizer"),
                );
            self.inner.unary(req, path, codec).await
        }
        pub async fn voice_cloning(
            &mut self,
            request: impl tonic::IntoRequest<super::VoiceCloningRequest>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| {
                    tonic::Status::unknown(
                        format!("Service was not ready: {}", e.into()),
                    )
                })?;
            let codec = tonic_prost::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static(
                "/ondewo.t2s.Text2Speech/VoiceCloning",
            );
            let mut req = request.into_request();
            req.extensions_mut()
                .insert(GrpcMethod::new("ondewo.t2s.Text2Speech", "VoiceCloning"));
            self.inner.unary(req, path, codec).await
        }
    }
}
/// Generated server implementations.
pub mod text2_speech_server {
    #![allow(
        unused_variables,
        dead_code,
        missing_docs,
        clippy::wildcard_imports,
        clippy::let_unit_value,
    )]
    use tonic::codegen::*;
    /// Generated trait containing gRPC methods that should be implemented for use with Text2SpeechServer.
    #[async_trait]
    pub trait Text2Speech: std::marker::Send + std::marker::Sync + 'static {
        async fn synthesize(
            &self,
            request: tonic::Request<super::SynthesizeRequest>,
        ) -> std::result::Result<
            tonic::Response<super::SynthesizeResponse>,
            tonic::Status,
        >;
        async fn batch_synthesize(
            &self,
            request: tonic::Request<super::BatchSynthesizeRequest>,
        ) -> std::result::Result<
            tonic::Response<super::BatchSynthesizeResponse>,
            tonic::Status,
        >;
        /// Server streaming response type for the StreamingSynthesize method.
        type StreamingSynthesizeStream: tonic::codegen::tokio_stream::Stream<
                Item = std::result::Result<
                    super::StreamingSynthesizeResponse,
                    tonic::Status,
                >,
            >
            + std::marker::Send
            + 'static;
        async fn streaming_synthesize(
            &self,
            request: tonic::Request<tonic::Streaming<super::StreamingSynthesizeRequest>>,
        ) -> std::result::Result<
            tonic::Response<Self::StreamingSynthesizeStream>,
            tonic::Status,
        >;
        async fn normalize_text(
            &self,
            request: tonic::Request<super::NormalizeTextRequest>,
        ) -> std::result::Result<
            tonic::Response<super::NormalizeTextResponse>,
            tonic::Status,
        >;
        async fn get_t2s_pipeline(
            &self,
            request: tonic::Request<super::T2sPipelineId>,
        ) -> std::result::Result<
            tonic::Response<super::Text2SpeechConfig>,
            tonic::Status,
        >;
        async fn create_t2s_pipeline(
            &self,
            request: tonic::Request<super::Text2SpeechConfig>,
        ) -> std::result::Result<tonic::Response<super::T2sPipelineId>, tonic::Status>;
        async fn delete_t2s_pipeline(
            &self,
            request: tonic::Request<super::T2sPipelineId>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status>;
        async fn update_t2s_pipeline(
            &self,
            request: tonic::Request<super::Text2SpeechConfig>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status>;
        async fn list_t2s_pipelines(
            &self,
            request: tonic::Request<super::ListT2sPipelinesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sPipelinesResponse>,
            tonic::Status,
        >;
        async fn list_t2s_languages(
            &self,
            request: tonic::Request<super::ListT2sLanguagesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sLanguagesResponse>,
            tonic::Status,
        >;
        async fn list_t2s_domains(
            &self,
            request: tonic::Request<super::ListT2sDomainsRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sDomainsResponse>,
            tonic::Status,
        >;
        async fn list_t2s_normalization_pipelines(
            &self,
            request: tonic::Request<super::ListT2sNormalizationPipelinesRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListT2sNormalizationPipelinesResponse>,
            tonic::Status,
        >;
        async fn get_service_info(
            &self,
            request: tonic::Request<()>,
        ) -> std::result::Result<
            tonic::Response<super::T2sGetServiceInfoResponse>,
            tonic::Status,
        >;
        async fn get_custom_phonemizer(
            &self,
            request: tonic::Request<super::PhonemizerId>,
        ) -> std::result::Result<
            tonic::Response<super::CustomPhonemizerProto>,
            tonic::Status,
        >;
        async fn create_custom_phonemizer(
            &self,
            request: tonic::Request<super::CreateCustomPhonemizerRequest>,
        ) -> std::result::Result<tonic::Response<super::PhonemizerId>, tonic::Status>;
        async fn delete_custom_phonemizer(
            &self,
            request: tonic::Request<super::PhonemizerId>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status>;
        async fn update_custom_phonemizer(
            &self,
            request: tonic::Request<super::UpdateCustomPhonemizerRequest>,
        ) -> std::result::Result<
            tonic::Response<super::CustomPhonemizerProto>,
            tonic::Status,
        >;
        async fn list_custom_phonemizer(
            &self,
            request: tonic::Request<super::ListCustomPhonemizerRequest>,
        ) -> std::result::Result<
            tonic::Response<super::ListCustomPhonemizerResponse>,
            tonic::Status,
        >;
        async fn voice_cloning(
            &self,
            request: tonic::Request<super::VoiceCloningRequest>,
        ) -> std::result::Result<tonic::Response<()>, tonic::Status>;
    }
    #[derive(Debug)]
    pub struct Text2SpeechServer<T> {
        inner: Arc<T>,
        accept_compression_encodings: EnabledCompressionEncodings,
        send_compression_encodings: EnabledCompressionEncodings,
        max_decoding_message_size: Option<usize>,
        max_encoding_message_size: Option<usize>,
    }
    impl<T> Text2SpeechServer<T> {
        pub fn new(inner: T) -> Self {
            Self::from_arc(Arc::new(inner))
        }
        pub fn from_arc(inner: Arc<T>) -> Self {
            Self {
                inner,
                accept_compression_encodings: Default::default(),
                send_compression_encodings: Default::default(),
                max_decoding_message_size: None,
                max_encoding_message_size: None,
            }
        }
        pub fn with_interceptor<F>(
            inner: T,
            interceptor: F,
        ) -> InterceptedService<Self, F>
        where
            F: tonic::service::Interceptor,
        {
            InterceptedService::new(Self::new(inner), interceptor)
        }
        /// Enable decompressing requests with the given encoding.
        #[must_use]
        pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.accept_compression_encodings.enable(encoding);
            self
        }
        /// Compress responses with the given encoding, if the client supports it.
        #[must_use]
        pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
            self.send_compression_encodings.enable(encoding);
            self
        }
        /// Limits the maximum size of a decoded message.
        ///
        /// Default: `4MB`
        #[must_use]
        pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
            self.max_decoding_message_size = Some(limit);
            self
        }
        /// Limits the maximum size of an encoded message.
        ///
        /// Default: `usize::MAX`
        #[must_use]
        pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
            self.max_encoding_message_size = Some(limit);
            self
        }
    }
    impl<T, B> tonic::codegen::Service<http::Request<B>> for Text2SpeechServer<T>
    where
        T: Text2Speech,
        B: Body + std::marker::Send + 'static,
        B::Error: Into<StdError> + std::marker::Send + 'static,
    {
        type Response = http::Response<tonic::body::Body>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;
        fn poll_ready(
            &mut self,
            _cx: &mut Context<'_>,
        ) -> Poll<std::result::Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            match req.uri().path() {
                "/ondewo.t2s.Text2Speech/Synthesize" => {
                    #[allow(non_camel_case_types)]
                    struct SynthesizeSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::SynthesizeRequest>
                    for SynthesizeSvc<T> {
                        type Response = super::SynthesizeResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::SynthesizeRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::synthesize(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = SynthesizeSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/BatchSynthesize" => {
                    #[allow(non_camel_case_types)]
                    struct BatchSynthesizeSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::BatchSynthesizeRequest>
                    for BatchSynthesizeSvc<T> {
                        type Response = super::BatchSynthesizeResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::BatchSynthesizeRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::batch_synthesize(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = BatchSynthesizeSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/StreamingSynthesize" => {
                    #[allow(non_camel_case_types)]
                    struct StreamingSynthesizeSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::StreamingService<super::StreamingSynthesizeRequest>
                    for StreamingSynthesizeSvc<T> {
                        type Response = super::StreamingSynthesizeResponse;
                        type ResponseStream = T::StreamingSynthesizeStream;
                        type Future = BoxFuture<
                            tonic::Response<Self::ResponseStream>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                tonic::Streaming<super::StreamingSynthesizeRequest>,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::streaming_synthesize(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = StreamingSynthesizeSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.streaming(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/NormalizeText" => {
                    #[allow(non_camel_case_types)]
                    struct NormalizeTextSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::NormalizeTextRequest>
                    for NormalizeTextSvc<T> {
                        type Response = super::NormalizeTextResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::NormalizeTextRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::normalize_text(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = NormalizeTextSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/GetT2sPipeline" => {
                    #[allow(non_camel_case_types)]
                    struct GetT2sPipelineSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::T2sPipelineId>
                    for GetT2sPipelineSvc<T> {
                        type Response = super::Text2SpeechConfig;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::T2sPipelineId>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::get_t2s_pipeline(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = GetT2sPipelineSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/CreateT2sPipeline" => {
                    #[allow(non_camel_case_types)]
                    struct CreateT2sPipelineSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::Text2SpeechConfig>
                    for CreateT2sPipelineSvc<T> {
                        type Response = super::T2sPipelineId;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::Text2SpeechConfig>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::create_t2s_pipeline(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = CreateT2sPipelineSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/DeleteT2sPipeline" => {
                    #[allow(non_camel_case_types)]
                    struct DeleteT2sPipelineSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::T2sPipelineId>
                    for DeleteT2sPipelineSvc<T> {
                        type Response = ();
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::T2sPipelineId>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::delete_t2s_pipeline(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = DeleteT2sPipelineSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/UpdateT2sPipeline" => {
                    #[allow(non_camel_case_types)]
                    struct UpdateT2sPipelineSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::Text2SpeechConfig>
                    for UpdateT2sPipelineSvc<T> {
                        type Response = ();
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::Text2SpeechConfig>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::update_t2s_pipeline(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = UpdateT2sPipelineSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/ListT2sPipelines" => {
                    #[allow(non_camel_case_types)]
                    struct ListT2sPipelinesSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::ListT2sPipelinesRequest>
                    for ListT2sPipelinesSvc<T> {
                        type Response = super::ListT2sPipelinesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ListT2sPipelinesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::list_t2s_pipelines(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = ListT2sPipelinesSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/ListT2sLanguages" => {
                    #[allow(non_camel_case_types)]
                    struct ListT2sLanguagesSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::ListT2sLanguagesRequest>
                    for ListT2sLanguagesSvc<T> {
                        type Response = super::ListT2sLanguagesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ListT2sLanguagesRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::list_t2s_languages(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = ListT2sLanguagesSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/ListT2sDomains" => {
                    #[allow(non_camel_case_types)]
                    struct ListT2sDomainsSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::ListT2sDomainsRequest>
                    for ListT2sDomainsSvc<T> {
                        type Response = super::ListT2sDomainsResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ListT2sDomainsRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::list_t2s_domains(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = ListT2sDomainsSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/ListT2sNormalizationPipelines" => {
                    #[allow(non_camel_case_types)]
                    struct ListT2sNormalizationPipelinesSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<
                        super::ListT2sNormalizationPipelinesRequest,
                    > for ListT2sNormalizationPipelinesSvc<T> {
                        type Response = super::ListT2sNormalizationPipelinesResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<
                                super::ListT2sNormalizationPipelinesRequest,
                            >,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::list_t2s_normalization_pipelines(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = ListT2sNormalizationPipelinesSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/GetServiceInfo" => {
                    #[allow(non_camel_case_types)]
                    struct GetServiceInfoSvc<T: Text2Speech>(pub Arc<T>);
                    impl<T: Text2Speech> tonic::server::UnaryService<()>
                    for GetServiceInfoSvc<T> {
                        type Response = super::T2sGetServiceInfoResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(&mut self, request: tonic::Request<()>) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::get_service_info(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = GetServiceInfoSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/GetCustomPhonemizer" => {
                    #[allow(non_camel_case_types)]
                    struct GetCustomPhonemizerSvc<T: Text2Speech>(pub Arc<T>);
                    impl<T: Text2Speech> tonic::server::UnaryService<super::PhonemizerId>
                    for GetCustomPhonemizerSvc<T> {
                        type Response = super::CustomPhonemizerProto;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::PhonemizerId>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::get_custom_phonemizer(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = GetCustomPhonemizerSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/CreateCustomPhonemizer" => {
                    #[allow(non_camel_case_types)]
                    struct CreateCustomPhonemizerSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::CreateCustomPhonemizerRequest>
                    for CreateCustomPhonemizerSvc<T> {
                        type Response = super::PhonemizerId;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::CreateCustomPhonemizerRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::create_custom_phonemizer(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = CreateCustomPhonemizerSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/DeleteCustomPhonemizer" => {
                    #[allow(non_camel_case_types)]
                    struct DeleteCustomPhonemizerSvc<T: Text2Speech>(pub Arc<T>);
                    impl<T: Text2Speech> tonic::server::UnaryService<super::PhonemizerId>
                    for DeleteCustomPhonemizerSvc<T> {
                        type Response = ();
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::PhonemizerId>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::delete_custom_phonemizer(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = DeleteCustomPhonemizerSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/UpdateCustomPhonemizer" => {
                    #[allow(non_camel_case_types)]
                    struct UpdateCustomPhonemizerSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::UpdateCustomPhonemizerRequest>
                    for UpdateCustomPhonemizerSvc<T> {
                        type Response = super::CustomPhonemizerProto;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::UpdateCustomPhonemizerRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::update_custom_phonemizer(
                                        &inner,
                                        request,
                                    )
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = UpdateCustomPhonemizerSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/ListCustomPhonemizer" => {
                    #[allow(non_camel_case_types)]
                    struct ListCustomPhonemizerSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::ListCustomPhonemizerRequest>
                    for ListCustomPhonemizerSvc<T> {
                        type Response = super::ListCustomPhonemizerResponse;
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::ListCustomPhonemizerRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::list_custom_phonemizer(&inner, request)
                                    .await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = ListCustomPhonemizerSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                "/ondewo.t2s.Text2Speech/VoiceCloning" => {
                    #[allow(non_camel_case_types)]
                    struct VoiceCloningSvc<T: Text2Speech>(pub Arc<T>);
                    impl<
                        T: Text2Speech,
                    > tonic::server::UnaryService<super::VoiceCloningRequest>
                    for VoiceCloningSvc<T> {
                        type Response = ();
                        type Future = BoxFuture<
                            tonic::Response<Self::Response>,
                            tonic::Status,
                        >;
                        fn call(
                            &mut self,
                            request: tonic::Request<super::VoiceCloningRequest>,
                        ) -> Self::Future {
                            let inner = Arc::clone(&self.0);
                            let fut = async move {
                                <T as Text2Speech>::voice_cloning(&inner, request).await
                            };
                            Box::pin(fut)
                        }
                    }
                    let accept_compression_encodings = self.accept_compression_encodings;
                    let send_compression_encodings = self.send_compression_encodings;
                    let max_decoding_message_size = self.max_decoding_message_size;
                    let max_encoding_message_size = self.max_encoding_message_size;
                    let inner = self.inner.clone();
                    let fut = async move {
                        let method = VoiceCloningSvc(inner);
                        let codec = tonic_prost::ProstCodec::default();
                        let mut grpc = tonic::server::Grpc::new(codec)
                            .apply_compression_config(
                                accept_compression_encodings,
                                send_compression_encodings,
                            )
                            .apply_max_message_size_config(
                                max_decoding_message_size,
                                max_encoding_message_size,
                            );
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        let mut response = http::Response::new(
                            tonic::body::Body::default(),
                        );
                        let headers = response.headers_mut();
                        headers
                            .insert(
                                tonic::Status::GRPC_STATUS,
                                (tonic::Code::Unimplemented as i32).into(),
                            );
                        headers
                            .insert(
                                http::header::CONTENT_TYPE,
                                tonic::metadata::GRPC_CONTENT_TYPE,
                            );
                        Ok(response)
                    })
                }
            }
        }
    }
    impl<T> Clone for Text2SpeechServer<T> {
        fn clone(&self) -> Self {
            let inner = self.inner.clone();
            Self {
                inner,
                accept_compression_encodings: self.accept_compression_encodings,
                send_compression_encodings: self.send_compression_encodings,
                max_decoding_message_size: self.max_decoding_message_size,
                max_encoding_message_size: self.max_encoding_message_size,
            }
        }
    }
    /// Generated gRPC service name
    pub const SERVICE_NAME: &str = "ondewo.t2s.Text2Speech";
    impl<T> tonic::server::NamedService for Text2SpeechServer<T> {
        const NAME: &'static str = SERVICE_NAME;
    }
}
