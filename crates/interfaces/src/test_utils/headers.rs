//! Testing support for headers related interfaces.
use crate::{
    consensus::{self, Consensus},
    p2p::{
        error::{RequestError, RequestResult},
        headers::{
            client::{HeadersClient, HeadersRequest},
            downloader::{HeaderBatchDownload, HeaderDownloadStream, HeaderDownloader},
            error::DownloadError,
        },
        traits::BatchDownload,
    },
};
use futures::{Future, FutureExt, Stream};
use reth_eth_wire::BlockHeaders;
use reth_primitives::{
    BlockLocked, BlockNumber, Header, HeadersDirection, SealedHeader, H256, U256,
};
use reth_rpc_types::engine::ForkchoiceState;
use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{ready, Context, Poll},
};
use tokio::sync::{watch, Mutex};

/// A test downloader which just returns the values that have been pushed to it.
#[derive(Debug)]
pub struct TestHeaderDownloader {
    client: Arc<TestHeadersClient>,
    consensus: Arc<TestConsensus>,
    limit: u64,
}

impl TestHeaderDownloader {
    /// Instantiates the downloader with the mock responses
    pub fn new(client: Arc<TestHeadersClient>, consensus: Arc<TestConsensus>, limit: u64) -> Self {
        Self { client, consensus, limit }
    }

    fn create_download(&self) -> TestDownload {
        TestDownload {
            client: Arc::clone(&self.client),
            consensus: Arc::clone(&self.consensus),
            limit: self.limit,
            fut: None,
            buffer: vec![],
            done: false,
        }
    }
}

#[async_trait::async_trait]
impl HeaderDownloader for TestHeaderDownloader {
    type Consensus = TestConsensus;
    type Client = TestHeadersClient;

    fn consensus(&self) -> &Self::Consensus {
        &self.consensus
    }

    fn client(&self) -> &Self::Client {
        &self.client
    }

    fn download(
        &self,
        _head: SealedHeader,
        _forkchoice: ForkchoiceState,
    ) -> HeaderBatchDownload<'_> {
        Box::pin(self.create_download())
    }

    fn stream(&self, _head: SealedHeader, _forkchoice: ForkchoiceState) -> HeaderDownloadStream {
        Box::pin(self.create_download())
    }
}

type TestHeadersFut = Pin<Box<dyn Future<Output = RequestResult<BlockHeaders>> + Send>>;

struct TestDownload {
    client: Arc<TestHeadersClient>,
    consensus: Arc<TestConsensus>,
    limit: u64,
    fut: Option<TestHeadersFut>,
    buffer: Vec<SealedHeader>,
    done: bool,
}

impl TestDownload {
    fn get_or_init_fut(&mut self) -> &mut TestHeadersFut {
        if self.fut.is_none() {
            let request = HeadersRequest {
                limit: self.limit,
                direction: HeadersDirection::Rising,
                start: reth_primitives::BlockHashOrNumber::Number(0), // ignored
            };
            let client = Arc::clone(&self.client);
            self.fut = Some(Box::pin(async move { client.get_headers(request).await }));
        }
        self.fut.as_mut().unwrap()
    }
}

impl Future for TestDownload {
    type Output = Result<Vec<SealedHeader>, DownloadError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let empty = SealedHeader::default();
        if let Err(error) = self.consensus.validate_header(&empty, &empty) {
            return Poll::Ready(Err(DownloadError::HeaderValidation { hash: empty.hash(), error }))
        }

        match ready!(self.get_or_init_fut().poll_unpin(cx)) {
            Ok(resp) => {
                // Skip head and seal headers
                let mut headers = resp.0.into_iter().skip(1).map(|h| h.seal()).collect::<Vec<_>>();
                headers.sort_unstable_by_key(|h| h.number);
                Poll::Ready(Ok(headers.into_iter().rev().collect()))
            }
            Err(err) => Poll::Ready(Err(match err {
                RequestError::Timeout => DownloadError::Timeout,
                _ => DownloadError::RequestError(err),
            })),
        }
    }
}

impl Stream for TestDownload {
    type Item = Result<SealedHeader, DownloadError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        loop {
            if let Some(header) = this.buffer.pop() {
                return Poll::Ready(Some(Ok(header)))
            } else if this.done {
                return Poll::Ready(None)
            }

            let empty = SealedHeader::default();
            if let Err(error) = this.consensus.validate_header(&empty, &empty) {
                this.done = true;
                return Poll::Ready(Some(Err(DownloadError::HeaderValidation {
                    hash: empty.hash(),
                    error,
                })))
            }

            match ready!(this.get_or_init_fut().poll_unpin(cx)) {
                Ok(resp) => {
                    // Skip head and seal headers
                    let mut headers =
                        resp.0.into_iter().skip(1).map(|h| h.seal()).collect::<Vec<_>>();
                    headers.sort_unstable_by_key(|h| h.number);
                    headers.into_iter().for_each(|h| this.buffer.push(h));
                    this.done = true;
                    continue
                }
                Err(err) => {
                    this.done = true;
                    return Poll::Ready(Some(Err(match err {
                        RequestError::Timeout => DownloadError::Timeout,
                        _ => DownloadError::RequestError(err),
                    })))
                }
            }
        }
    }
}

impl BatchDownload for TestDownload {
    type Ok = SealedHeader;
    type Error = DownloadError;

    fn into_stream_unordered(self) -> Box<dyn Stream<Item = Result<Self::Ok, Self::Error>>> {
        Box::new(self)
    }
}

/// A test client for fetching headers
#[derive(Debug, Default)]
pub struct TestHeadersClient {
    responses: Arc<Mutex<Vec<Header>>>,
    error: Arc<Mutex<Option<RequestError>>>,
}

impl TestHeadersClient {
    /// Adds headers to the set.
    pub async fn extend(&self, headers: impl IntoIterator<Item = Header>) {
        let mut lock = self.responses.lock().await;
        lock.extend(headers);
    }

    /// Set repsonse error
    pub async fn set_error(&self, err: RequestError) {
        let mut lock = self.error.lock().await;
        lock.replace(err);
    }
}

#[async_trait::async_trait]
impl HeadersClient for TestHeadersClient {
    fn update_status(&self, _height: u64, _hash: H256, _td: U256) {}

    async fn get_headers(&self, request: HeadersRequest) -> RequestResult<BlockHeaders> {
        if let Some(err) = &mut *self.error.lock().await {
            return Err(err.clone())
        }

        let mut lock = self.responses.lock().await;
        let len = lock.len().min(request.limit as usize);
        let resp = lock.drain(..len).collect();
        return Ok(BlockHeaders(resp))
    }
}

/// Consensus engine implementation for testing
#[derive(Debug)]
pub struct TestConsensus {
    /// Watcher over the forkchoice state
    channel: (watch::Sender<ForkchoiceState>, watch::Receiver<ForkchoiceState>),
    /// Flag whether the header validation should purposefully fail
    fail_validation: AtomicBool,
}

impl Default for TestConsensus {
    fn default() -> Self {
        Self {
            channel: watch::channel(ForkchoiceState {
                head_block_hash: H256::zero(),
                finalized_block_hash: H256::zero(),
                safe_block_hash: H256::zero(),
            }),
            fail_validation: AtomicBool::new(false),
        }
    }
}

impl TestConsensus {
    /// Update the fork choice state
    pub fn update_tip(&self, tip: H256) {
        let state = ForkchoiceState {
            head_block_hash: tip,
            finalized_block_hash: H256::zero(),
            safe_block_hash: H256::zero(),
        };
        self.channel.0.send(state).expect("updating fork choice state failed");
    }

    /// Get the failed validation flag
    pub fn fail_validation(&self) -> bool {
        self.fail_validation.load(Ordering::SeqCst)
    }

    /// Update the validation flag
    pub fn set_fail_validation(&self, val: bool) {
        self.fail_validation.store(val, Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Consensus for TestConsensus {
    fn fork_choice_state(&self) -> watch::Receiver<ForkchoiceState> {
        self.channel.1.clone()
    }

    fn validate_header(
        &self,
        _header: &SealedHeader,
        _parent: &SealedHeader,
    ) -> Result<(), consensus::Error> {
        if self.fail_validation() {
            Err(consensus::Error::BaseFeeMissing)
        } else {
            Ok(())
        }
    }

    fn pre_validate_block(&self, _block: &BlockLocked) -> Result<(), consensus::Error> {
        if self.fail_validation() {
            Err(consensus::Error::BaseFeeMissing)
        } else {
            Ok(())
        }
    }
    fn has_block_reward(&self, _block_num: BlockNumber) -> bool {
        true
    }
}
