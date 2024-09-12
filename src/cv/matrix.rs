use opencv::{core, prelude::*};
pub struct Matrix(core::Mat);

impl Matrix {
    pub fn resize(&self, size: (usize, usize)) -> crate::Result<Self> {
        let mut new_mat = core::Mat::default();
        opencv::imgproc::resize(
            &self.0,
            &mut new_mat,
            core::Size_::new(size.0 as i32, size.1 as i32),
            0.,
            0.,
            opencv::imgproc::INTER_LINEAR,
        )
        .map_err(crate::Error::OpenCVError)?;
        Ok(Matrix(new_mat))
    }
}

impl From<core::Mat> for Matrix {
    fn from(value: core::Mat) -> Self {
        Self(value)
    }
}

impl From<Matrix> for eframe::egui::ImageData {
    fn from(value: Matrix) -> Self {
        //HANDLE ERR
        let size = value.size().unwrap_or_default();
        eframe::egui::ImageData::Color(std::sync::Arc::new(eframe::egui::ColorImage {
            size: [size.width as usize, size.height as usize],
            pixels: value
                .data_bytes()
                .unwrap_or(&vec![0; (size.width * size.height) as usize])
                .chunks_exact(3)
                // OPENCV BGR -> RGB
                .map(|p| eframe::egui::Color32::from_rgba_premultiplied(p[2], p[1], p[0], u8::MAX))
                .collect(),
        }))
    }
}

impl From<Matrix> for crate::processor::TensorData {
    fn from(value: Matrix) -> Self {
        let size = value.size().unwrap_or_default();
        let binding = vec![0; (size.width * size.height) as usize];
        let bytes = value.data_bytes().unwrap_or(&binding);
        ndarray::Array::from_shape_fn(
            (1, 3, size.width as usize, size.height as usize),
            |(_, c, x, y)| {
                ((bytes[3 * x + 3 * y * (size.width as usize) + c] as f32) - 127.5) / 127.5
            }, // u8::MAX / 2
        )
    }
}

impl std::ops::Deref for Matrix {
    type Target = core::Mat;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Matrix {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod test {
    use opencv::core::MatTraitConst;

    use super::Matrix;

    #[test]
    fn properly_converts_matrix_to_img_data() {
        let core_mat = opencv::core::Mat::from_bytes::<u8>(&[1, 2, 2, 5, 4, 3])
            .expect("Failed to create mat")
            .clone_pointee()
            .reshape_def(3)
            .expect("Failed to set channel count")
            .clone_pointee();
        let matrix = Matrix(core_mat);
        let img_data = eframe::egui::ImageData::from(matrix);
        let size = img_data.size();
        assert_eq!(size, [2, 1]);
    }

    #[test]
    fn matrix_contain_correct_bytes_on_resize() {
        let test_mat = Matrix::from(
            opencv::core::Mat::new_rows_cols_with_default(
                200,
                300,
                opencv::core::CV_8UC3,
                opencv::core::Scalar::default(),
            )
            .expect("Failed to create test matrix"),
        );
        let new_sized_test_mat = test_mat
            .resize((150, 125))
            .expect("Failed to resize test matrix")
            .size()
            .unwrap();

        assert_eq!(
            new_sized_test_mat.width * new_sized_test_mat.height,
            150 * 125
        );
    }

    // #[test]
    // fn properly_converts_matrix_to_ndarray() {
    //     let array = ndarray::Array::<u8, ndarray::Dim<[usize; 4]>>::zeros((1, 3, 4, 4));
    //     assert_eq!(
    //         array,
    //         ndarray::array![[
    //             [[0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
    //             [[0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
    //             [[0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]
    //         ]]
    //     )
    // }
}
