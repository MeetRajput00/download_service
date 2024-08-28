use std::{io::{Cursor,Read,Write}};
use std::time::Instant;
use reqwest::Client;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Mutex;
use std::sync::Arc;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

enum DownloadType{
    SegmentedDownload,
    SequentialDownload
}
trait IDownloadService {
    async fn download_segment(&self,start:u64,end:u64)->std::io::Result<bytes::Bytes>;
    async fn download_file(&self) -> Result<()>;
    fn set_file_name(&mut self,file_name:String) -> Result<()>;
    async fn get_file_size(&self) -> std::io::Result<u64>;
    async fn start_download(&self,download_type:DownloadType);
}
struct MyDownloadService{
    url:String,
    client:Client,
    max_parallelism:u64,
    file_name:String
}
impl IDownloadService for MyDownloadService{
    async fn start_download(&self,download_type:DownloadType) {
        match download_type {
            DownloadType::SegmentedDownload=>{
                let file_size=self.get_file_size().await.unwrap_or_default();

                let max_parallelism=self.max_parallelism.clone();
                let segment_size=file_size/max_parallelism;

                let mut handles=vec![];
                let file = Arc::new(Mutex::new(
                    tokio::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(self.file_name.clone())
                        .await.unwrap(),
                ));
                for i in 0..max_parallelism{
                    let start_byte = i * segment_size;
                    let end_byte = if i == max_parallelism - 1 {
                        file_size - 1
                    } else {
                        start_byte + segment_size - 1
                    };
                    // Clone the components needed in the async block
                    let url = self.url.clone();
                    let client = self.client.clone();
                    let file_clone = Arc::clone(&file);

                    let handle = tokio::spawn(async move {
                        // Download the segment
                        let response = client
                            .get(&url)
                            .header("Range", format!("bytes={}-{}", start_byte, end_byte))
                            .send()
                            .await
                            .unwrap()
                            .bytes()
                            .await
                            .unwrap();

                        // Write to the file
                        let mut file = file_clone.lock().await;
                        file.seek(std::io::SeekFrom::Start(start_byte)).await.unwrap();
                        file.write_all(&response).await.unwrap();
                    });

                    handles.push(handle);
                }
                futures::future::join_all(handles).await;
            },
            DownloadType::SequentialDownload=>{
                self.download_file().await.unwrap();
            }
        }
    }
    async fn download_segment(&self,start:u64,end:u64)->std::io::Result<bytes::Bytes>{
        let range = format!("bytes={}-{}", start, end);
        let response = self.client
            .get(self.url.clone())
            .header("Range", range)
            .send()
            .await.unwrap()
            .bytes()
            .await.unwrap();
    
        Ok(response)
    }
    
    async fn download_file(&self) -> Result<()> {
        let response = reqwest::get(self.url.clone()).await?;
        let file = std::fs::File::create(self.file_name.clone())?;
        let mut content =  Cursor::new(response.bytes().await?);
        let mut buffered_writer=std::io::BufWriter::new(file);
        let mut buffer=vec![0;8192];
        loop{
            let bytes_read=content.read(&mut buffer)?;
            if bytes_read==0{
                break;
            }
            buffered_writer.write_all(&buffer[..bytes_read])?;
        }
        buffered_writer.flush()?;
        Ok(())
    }
    async fn get_file_size(&self) -> std::io::Result<u64> {
        let response = self.client.head(self.url.clone()).send().await.unwrap();
        let content_length = response
            .headers()
            .get("Content-Length")
            .ok_or("No Content-Length header").unwrap()
            .to_str().unwrap()
            .parse::<u64>().unwrap();
    
        Ok(content_length)
    }
    fn set_file_name(&mut self,file_name:String) -> Result<()> {
        self.file_name=file_name;
        return Ok(());
    }
}
#[tokio::main]
async fn main() {
    let mut download=MyDownloadService{
        url:"https://gbihr.org/images/docs/test.pdf".to_string(),
        client:Client::new(),
        max_parallelism:4,
        file_name:"demo_file".to_string()
    };
    download.set_file_name("seq_file.pdf".to_string());
    let seqTimer=Instant::now();
    download.start_download(DownloadType::SequentialDownload).await;
    println!("Seq download time: {:?}",seqTimer.elapsed());
    download.set_file_name("seg_file.pdf".to_string());
    let segTimer=Instant::now();
    download.start_download(DownloadType::SegmentedDownload).await;
    println!("Seg download time: {:?}",segTimer.elapsed());
}