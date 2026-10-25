use std::pin::Pin;

pub type ChunkRead<'source, Chunk, ReadError> =
    Pin<Box<dyn Future<Output = Result<Option<Chunk>, ReadError>> + Send + 'source>>;

#[derive(Debug, PartialEq, Eq)]
pub enum CappedReadError<ReadError> {
    Read(ReadError),
    TooLarge,
}

pub async fn read_capped<Source, Chunk: AsRef<[u8]>, ReadError>(
    source: &mut Source,
    read_chunk: impl for<'source> Fn(&'source mut Source) -> ChunkRead<'source, Chunk, ReadError>,
    max_bytes: usize,
) -> Result<Vec<u8>, CappedReadError<ReadError>> {
    let mut body = Vec::new();
    while let Some(chunk) = read_chunk(source).await.map_err(CappedReadError::Read)? {
        let chunk = chunk.as_ref();
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(CappedReadError::TooLarge);
        }
        body.extend_from_slice(chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    use super::*;

    type Chunks = VecDeque<Result<&'static str, &'static str>>;

    fn resolved<Output>(future: impl Future<Output = Output>) -> Output {
        match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("the test future did not complete"),
        }
    }

    fn next_chunk(chunks: &mut Chunks) -> ChunkRead<'_, &'static str, &'static str> {
        let chunk = chunks.pop_front().transpose();
        Box::pin(async move { chunk })
    }

    fn read_chunks(
        chunks: &[Result<&'static str, &'static str>],
        max_bytes: usize,
    ) -> Result<Vec<u8>, CappedReadError<&'static str>> {
        let mut pending: Chunks = chunks.iter().copied().collect();
        resolved(read_capped(&mut pending, next_chunk, max_bytes))
    }

    #[test]
    fn given_chunks_within_the_cap_when_reading_then_they_are_joined() {
        assert_eq!(read_chunks(&[Ok("ab"), Ok("cd")], 4), Ok(b"abcd".to_vec()));
        assert_eq!(read_chunks(&[], 0), Ok(Vec::new()));
    }

    #[test]
    fn given_chunks_over_the_cap_when_reading_then_it_is_too_large() {
        assert_eq!(
            read_chunks(&[Ok("ab"), Ok("cde")], 4),
            Err(CappedReadError::TooLarge)
        );
    }

    #[test]
    fn given_a_failing_read_when_reading_then_the_failure_is_returned() {
        assert_eq!(
            read_chunks(&[Ok("ab"), Err("broken")], 4),
            Err(CappedReadError::Read("broken"))
        );
    }
}
