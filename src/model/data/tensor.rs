// (n, c, h, w)
pub type TensorData = ndarray::Array<f32, ndarray::Dim<[usize; 4]>>;

#[derive(Debug, PartialEq, Clone)]
pub enum Normal {
    /// Negative One To Plus One
    N1ToP1,
    /// Zero to Plus One
    ZeroToP1,
    /// Zero to 255
    U8,
}

#[derive(Debug, Clone)]
pub struct Tensor {
    pub normal: Normal,
    pub data: TensorData,
}

impl Default for Tensor {
    fn default() -> Self {
        Self {
            normal: Normal::N1ToP1,
            data: ndarray::Array::zeros((1, 3, 128, 128)),
        }
    }
}

impl Tensor {
    pub fn new(normal: Normal, array: TensorData) -> Self {
        Self {
            normal,
            data: array,
        }
    }

    pub fn is_eq_dim(&self, cmp_dim: (usize, usize, usize, usize)) -> bool {
        let dim = self.dim();
        dim.0 == cmp_dim.0 && dim.1 == cmp_dim.1 && dim.2 == cmp_dim.2 && dim.3 == cmp_dim.3
    }

    pub fn to_normalization(&mut self, n: Normal) {
        let curr_normalization = self.normal.clone();
        if curr_normalization == n {
            return;
        }
        self.par_mapv_inplace(|v| match curr_normalization {
            Normal::N1ToP1 => match n {
                Normal::ZeroToP1 => v / 2. + 0.5,
                Normal::U8 => v * 127.5 + 127.5,
                Normal::N1ToP1 => v,
            },
            Normal::ZeroToP1 => match n {
                Normal::N1ToP1 => v * 2. - 1.,
                Normal::U8 => v * 255.,
                Normal::ZeroToP1 => v,
            },
            Normal::U8 => match n {
                Normal::N1ToP1 => (v - 127.5) / 127.5,
                Normal::ZeroToP1 => v / 255.,
                Normal::U8 => v,
            },
        });
        self.normal = n;
    }

    pub fn resize(&self, size: (usize, usize)) -> Self {
        let (_, _, cur_y, cur_x) = self.dim();
        if cur_x == size.0 && cur_y == size.1 {
            return self.clone();
        }
        let (x_scale_factor, y_scale_factor) = (
            if size.0 != 0 {
                cur_x as f32 / size.0 as f32
            } else {
                0.
            },
            if size.1 != 0 {
                cur_y as f32 / size.1 as f32
            } else {
                0.
            },
        );

        let new_arr = ndarray::Zip::from(&mut ndarray::Array::<
            (usize, usize, usize, usize),
            ndarray::Dim<[usize; 4]>,
        >::from_shape_fn(
            (1, 3, size.1, size.0), |dim| dim
        ))
        .par_map_collect(|(n, c, y, x)| {
            // new x & new y
            let (nx, ny) = ((*x as f32) * x_scale_factor, (*y as f32) * y_scale_factor);
            let (x_floor, x_ceil) = (
                nx.floor() as usize,
                std::cmp::min(nx.ceil() as usize, cur_x - 1),
            );
            let (y_floor, y_ceil) = (
                ny.floor() as usize,
                std::cmp::min(ny.ceil() as usize, cur_y - 1),
            );

            if x_ceil == x_floor && y_ceil == y_floor {
                return self[(*n, *c, ny as usize, nx as usize)];
            }

            if x_ceil == x_floor {
                let (q1, q2) = (
                    self[(*n, *c, y_floor, nx as usize)],
                    self[(*n, *c, y_ceil, nx as usize)],
                );
                return q1 * (y_ceil as f32 - ny) + q2 * (ny - y_floor as f32);
            }

            if y_ceil == y_floor {
                let (q1, q2) = (
                    self[(*n, *c, ny as usize, x_floor)],
                    self[(*n, *c, ny as usize, x_ceil)],
                );
                return q1 * (x_ceil as f32 - nx) + q2 * (nx - x_floor as f32);
            }

            // corner values
            let (v1, v2, v3, v4) = (
                self[(*n, *c, y_floor, x_floor)],
                self[(*n, *c, y_floor, x_ceil)],
                self[(*n, *c, y_ceil, x_floor)],
                self[(*n, *c, y_ceil, x_ceil)],
            );
            let (q1, q2) = (
                v1 * (x_ceil as f32 - nx) + v2 * (nx - x_floor as f32),
                v3 * (x_ceil as f32 - nx) + v4 * (nx - x_floor as f32),
            );
            q1 * (y_ceil as f32 - ny) + q2 * (ny - y_floor as f32)
        });

        Self {
            normal: self.normal.clone(),
            data: new_arr,
        }
    }

    pub fn to_cuda_slice(
        self,
        cuda: &std::sync::Arc<cudarc::driver::CudaDevice>,
    ) -> crate::Result<cudarc::driver::CudaSlice<f32>> {
        cuda.htod_sync_copy(&self.data.into_raw_vec_and_offset().0)
            .map_err(crate::Error::CudaError)
    }

    pub fn mean(&self) -> f32 {
        let (_, c, y, x) = self.dim();
        self.data.flatten().sum() / (c * y * x) as f32
    }

    // use rayon par iter
    pub fn norm(&self) -> f32 {
        self.flatten().map(|v| v * v).sum().sqrt()
    }
}

impl From<TensorData> for Tensor {
    fn from(value: TensorData) -> Self {
        Self {
            normal: Normal::ZeroToP1,
            data: value,
        }
    }
}

impl From<Tensor> for TensorData {
    fn from(value: Tensor) -> Self {
        value.data
    }
}

impl From<Tensor> for eframe::egui::ImageData {
    fn from(value: Tensor) -> Self {
        use eframe::egui::{Color32, ColorImage, ImageData};
        use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

        let (multiplier, norm_add) = (
            match value.normal {
                Normal::N1ToP1 => 127.5,
                Normal::ZeroToP1 => 255.,
                Normal::U8 => 1.,
            },
            if value.normal == Normal::N1ToP1 {
                127.5
            } else {
                0.
            },
        );

        let (_, _, height, width) = value.dim();
        ImageData::Color(std::sync::Arc::new(ColorImage {
            size: [width, height],
            pixels: (0..width * height)
                .collect::<Vec<usize>>()
                .par_iter()
                .map(|i| {
                    let (x, y) = (i % width, i / width);
                    Color32::from_rgba_premultiplied(
                        (value[[0, 0, y, x]] * multiplier + norm_add) as u8,
                        (value[[0, 1, y, x]] * multiplier + norm_add) as u8,
                        (value[[0, 2, y, x]] * multiplier + norm_add) as u8,
                        255,
                    )
                })
                .collect(),
        }))
    }
}

impl From<Tensor> for crate::image::Image {
    fn from(mut value: Tensor) -> Self {
        value.to_normalization(Normal::U8);
        let (_, _, height, width) = value.dim();

        let (multiplier, norm_add) = (
            match value.normal {
                Normal::N1ToP1 => 127.5,
                Normal::ZeroToP1 => 255.,
                Normal::U8 => 1.,
            },
            if value.normal == Normal::N1ToP1 {
                127.5
            } else {
                0.
            },
        );
        crate::image::Image::from(image::RgbImage::from_par_fn(
            width as u32,
            height as u32,
            |x, y| {
                image::Rgb([
                    (value[[0, 0, y as usize, x as usize]] * multiplier + norm_add) as u8,
                    (value[[0, 1, y as usize, x as usize]] * multiplier + norm_add) as u8,
                    (value[[0, 2, y as usize, x as usize]] * multiplier + norm_add) as u8,
                ])
            },
        ))
    }
}

impl std::ops::Deref for Tensor {
    type Target = TensorData;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl std::ops::DerefMut for Tensor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

#[cfg(test)]
mod test {
    use crate::model::TensorData;

    use super::{Normal, Tensor};
    use rand::Rng;

    #[test]
    fn can_convert_tensor_data_to_image() {
        let mut rand = rand::thread_rng();
        let (w, h) = (128, 128);
        let tensor = Tensor::from(TensorData::from_shape_fn((1, 3, w, h), |_| rand.gen()));
        let tensor_img = crate::image::Image::from(tensor.clone());
        let (rand_x, rand_y, rand_c) = (
            rand.gen_range(0..w),
            rand.gen_range(0..h),
            rand.gen_range(0..3),
        );

        assert_eq!(
            (tensor[(0, rand_c, rand_y, rand_x)] * 255.) as u8,
            tensor_img[(rand_x as u32, rand_y as u32)][rand_c],
        );
    }

    #[test]
    fn can_resize_tensor_data() {
        let mut rand = rand::thread_rng();
        let test_data = Tensor::default();
        let new_size = (
            rand.gen_range(0..1000) as usize,
            rand.gen_range(0..1000) as usize,
        );

        let resized_data = test_data.resize(new_size);
        let (_, _, new_y, new_x) = resized_data.dim();

        assert_eq!(new_x, new_size.0, "resized width doesn't match");
        assert_eq!(new_y, new_size.1, "resized height doesn't match");

        assert_eq!(
            new_size.0 * new_size.1 * 3,
            resized_data.flatten().len(),
            "resized tensor byte length doesn't match"
        );
    }

    #[test]
    fn can_convert_tensor_normalization() {
        let mut rand = rand::thread_rng();
        let (w, h) = (100, 100);
        let (rand_x, rand_y, rand_c) = (
            rand.gen_range(0..w),
            rand.gen_range(0..h),
            rand.gen_range(0..3),
        );

        for mut t in [
            Tensor::new(
                Normal::ZeroToP1,
                TensorData::from_shape_fn((1, 3, h, w), |_| rand.gen()),
            ),
            Tensor::new(
                Normal::N1ToP1,
                TensorData::from_shape_fn((1, 3, h, w), |_| rand.gen::<f32>() * 2. - 1.),
            ),
            Tensor::new(
                Normal::U8,
                TensorData::from_shape_fn((1, 3, h, w), |_| rand.gen_range(0..=255) as f32),
            ),
        ] {
            let normalization = t.normal.clone();
            let t_val = t.data[(0, rand_c, rand_y, rand_x)];
            if normalization == Normal::ZeroToP1 {
                t.to_normalization(Normal::N1ToP1);
                assert_eq!(
                    t_val * 2. - 1.,
                    t.data[(0, rand_c, rand_y, rand_x)],
                    "ZeroToP1 to N1ToP1"
                );
                t.to_normalization(normalization.clone());
                t.to_normalization(Normal::U8);
                assert_eq!(
                    t_val * 255.,
                    t.data[(0, rand_c, rand_y, rand_x)],
                    "ZeroToP1 to U8"
                );
            } else if normalization == Normal::N1ToP1 {
                t.to_normalization(Normal::ZeroToP1);
                assert_eq!(
                    t_val / 2. + 0.5,
                    t.data[(0, rand_c, rand_y, rand_x)],
                    "N1ToP1 to ZeroToP1"
                );
                t.to_normalization(normalization.clone());
                t.to_normalization(Normal::U8);
                assert_eq!(
                    t_val * 127.5 + 127.5,
                    t.data[(0, rand_c, rand_y, rand_x)],
                    "N1ToP1 to U8"
                );
            } else {
                // u8
                t.to_normalization(Normal::ZeroToP1);
                assert_eq!(
                    t_val / 255.,
                    t.data[(0, rand_c, rand_y, rand_x)],
                    "U8 to ZeroToP1"
                );
                t.to_normalization(normalization.clone());
                t.to_normalization(Normal::N1ToP1);
                assert_eq!(
                    (t_val - 127.5) / 127.5,
                    t.data[(0, rand_c, rand_y, rand_x)],
                    "U8 to N1ToP1"
                );
            }
        }
    }
}
