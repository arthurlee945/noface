use cudarc::driver::CudaDevice;
use detection_model::DetectionModel;
use recognition_model::RecognitionModel;
use swap_model::SwapModel;

use crate::{Error, Result};
pub use data::{ModelData, RecgnData, TensorData};

mod detection_model;
mod recognition_model;
mod swap_model;

pub mod data;

type ArcCudaDevice = std::sync::Arc<cudarc::driver::CudaDevice>;
// extend to use get face location + embed swap face
// https://github.com/pykeio/ort/blob/main/examples/cudarc/src/main.rs
// https://onnxruntime.ai/docs/install/
pub struct Model {
    #[allow(dead_code)]
    detect: DetectionModel,
    swap: SwapModel,
    recgn: RecognitionModel,
    cuda: Option<ArcCudaDevice>,
}

impl Model {
    //might want thread count etc from config
    #[tracing::instrument(name = "Initializing Models", skip(config), err)]
    pub fn new(config: &crate::setting::ModelConfig) -> Result<Self> {
        let model_base_path = std::env::current_dir()
            .map_err(Error::as_unknown_error)?
            .join("models");

        Ok(Self {
            detect: DetectionModel::new(model_base_path.join("det_10g.onnx"))?,
            swap: SwapModel::new(model_base_path.join("inswapper_128.onnx"))?,
            recgn: RecognitionModel::new(model_base_path.join("w600k_r50.onnx"))?,
            cuda: config
                .cuda
                .then_some(cudarc::driver::CudaDevice::new(0).map_err(Error::CudaError)?),
        })
    }

    pub fn run(&self, tar: TensorData, src: TensorData) -> Result<TensorData> {
        // let (tar_faces, src_face) = rayon::join(
        //     || self.detect.run(tar.clone(), self.cuda.as_ref()),
        //     || self.detect.run(src.clone(), self.cuda.as_ref()),
        // );

        let _ = self.detect.run(src.clone(), self.cuda.as_ref());
        // let recgn_data = self.recgn.run(src, self.cuda.as_ref())?;
        // self.swap.run(tar, recgn_data, self.cuda.as_ref())
        Ok(tar)
    }
}

#[tracing::instrument(err)]
pub fn register_ort(config: &crate::setting::ModelConfig) -> Result<()> {
    let onnx_env = ort::init().with_name("noface_image_procesor");

    let onnx_env = match config.cuda {
        true => onnx_env.with_execution_providers([ort::CUDAExecutionProvider::default()
            .build()
            .error_on_failure()]),
        false => onnx_env,
    };

    onnx_env.commit().map_err(Error::ModelError)?;
    Ok(())
}

fn start_session_from_file(onnx_path: std::path::PathBuf) -> Result<ort::Session> {
    ort::Session::builder()
        .map_err(Error::ModelError)?
        .with_intra_threads(4)
        .map_err(Error::ModelError)?
        .commit_from_file(onnx_path)
        .map_err(Error::ModelError)
}
