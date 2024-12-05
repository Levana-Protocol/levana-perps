use crate::market_param::HistoricalData;
use anyhow::Context;
use std::path::Path;

#[derive(Clone)]
pub(crate) struct S3 {
    pub(crate) client: aws_sdk_s3::Client,
    pub(crate) bucket: String,
}

impl S3 {
    pub(crate) async fn new(bucket: String) -> anyhow::Result<Self> {
        let config = aws_config::load_from_env().await;

        Ok(Self {
            client: aws_sdk_s3::Client::new(&config),
            bucket,
        })
    }

    pub(crate) async fn upload(&self, path: &Path, data: &HistoricalData) -> anyhow::Result<()> {
        let market_data =
            serde_json::to_vec(&data).context("Could not serialize Historical Data to JSON")?;
        match path.to_str() {
            Some(key_path) => {
                let body = aws_sdk_s3::primitives::ByteStream::from(market_data);

                self.client
                    .put_object()
                    .bucket(self.bucket.clone())
                    .body(body)
                    .key(key_path)
                    .send()
                    .await
                    .context("Failed uploading file to s3")?;

                Ok(())
            }
            None => Err(anyhow::Error::msg("Error uploading file, invalid path")),
        }
    }

    pub(crate) async fn download(&self, path: &Path) -> anyhow::Result<HistoricalData> {
        match path.to_str() {
            Some(key_path) => {
                let object = self
                    .client
                    .get_object()
                    .bucket(self.bucket.clone())
                    .key(key_path)
                    .send()
                    .await
                    .context("Failed downloading file from S3")?;

                let stream = object.body;
                let bytes = stream.collect().await?.into_bytes();
                let historical_data: HistoricalData = serde_json::from_slice(&bytes)
                    .context("Error deserializing Historical Data from S3")?;

                Ok(historical_data)
            }
            None => Err(anyhow::Error::msg("Error downloading file, invalid path")),
        }
    }
}
