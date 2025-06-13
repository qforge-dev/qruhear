#![allow(dead_code)]
use anyhow::Result;
use screencapturekit::shareable_content::{SCDisplay, SCShareableContent};
use screencapturekit::stream::content_filter::SCContentFilter;
use screencapturekit::stream::output_type::SCStreamOutputType;
use screencapturekit::stream::output_trait::SCStreamOutputTrait;
use screencapturekit::stream::delegate_trait::SCStreamDelegateTrait;
use screencapturekit::stream::{SCStream};
use screencapturekit::output::CMSampleBuffer;
use screencapturekit::stream::configuration::SCStreamConfiguration;
use core_foundation::error::CFError;

use std::sync::{Arc, Mutex};

type RUBuffers = Vec<Vec<f32>>;

struct ErrorHandler;

impl SCStreamDelegateTrait for ErrorHandler {
    fn did_stop_with_error(&self, _stream: SCStream, error: CFError) {
        println!("Stream Error: {:?}", error);
    }
}

struct OutputHandler {
    callback: Arc<Mutex<dyn FnMut(RUBuffers) + Send>>,
}

impl SCStreamOutputTrait for OutputHandler {
    fn did_output_sample_buffer(
        &self,
        sample_buffer: CMSampleBuffer,
        _of_type: SCStreamOutputType,
    ) {
        let audio = sample_buffer.get_audio_buffer_list().unwrap();
        let mut audio_buffers = Vec::new();
        for channel_index in 0..audio.num_buffers() {
            let channel = audio.get(channel_index).unwrap();
            let data = channel.data();
            let mut f32_data = Vec::new();
            for i in 0..data.len() / 4 {
                let mut f32_bytes = [0u8; 4];
                f32_bytes.copy_from_slice(&data[i * 4..i * 4 + 4]);
                let f32 = f32::from_le_bytes(f32_bytes);
                f32_data.push(f32);
            }
            audio_buffers.push(f32_data);
        }
        let mut callback = self.callback.lock().unwrap();
        callback(audio_buffers);
    }
}

#[derive(Debug, Clone)]
enum RUDevice {
    MacosDisplay(SCDisplay),
    // MacosApplication(SCRunningApplication),
    // MacosWindow(SCWindow),
}

pub struct RUHear {
    pub callback: Arc<Mutex<dyn FnMut(RUBuffers) + Send>>,
    device_list: Vec<RUDevice>,
    device: Option<RUDevice>,
    stream: Option<SCStream>,
}

impl RUHear {
    pub fn new(callback: Arc<Mutex<dyn FnMut(RUBuffers) + Send>>) -> Self {
        let content = SCShareableContent::get().unwrap();
        let displays = content.displays();
        let display = displays.first().unwrap().to_owned();
        Self {
            callback: callback.clone(),
            device_list: displays
                .iter()
                .map(|display| RUDevice::MacosDisplay(display.clone()))
                .collect(),
            device: Some(RUDevice::MacosDisplay(display.clone())),
            stream: None,
        }
    }

    pub fn start(&mut self) -> Result<(), anyhow::Error> {
        let display = match self.device.clone().unwrap() {
            RUDevice::MacosDisplay(display) => display,
        };
        
        let filter = SCContentFilter::new().with_display_excluding_windows(&display, &[]);
        let config = SCStreamConfiguration::new()
            .set_captures_audio(true)
            .map_err(|e| anyhow::anyhow!("Failed to set captures_audio: {:?}", e))?;
        
        let mut stream = SCStream::new(&filter, &config);
        let output_handler = OutputHandler {
            callback: self.callback.clone(),
        };
        stream.add_output_handler(output_handler, SCStreamOutputType::Audio);
        
        let result = stream.start_capture();
        self.stream = Some(stream);
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Failed to start capture: {:?}", e)),
        }
    }

    pub fn stop(&mut self) -> Result<(), anyhow::Error> {
        if let Some(stream) = self.stream.take() {
            match stream.stop_capture() {
                Ok(_) => Ok(()),
                Err(e) => Err(anyhow::anyhow!("Failed to stop capture: {:?}", e)),
            }
        } else {
            anyhow::bail!("Stream not found")
        }
    }
}
