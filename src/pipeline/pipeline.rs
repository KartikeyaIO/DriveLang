use crate::Frame;
use crate::filter::Filter;
use crate::media::frame::{PixelData, Pos};

pub trait Pipeline {
    fn apply(frame: &mut Frame, filter: &Filter) -> Result<(), PipelineError> {
        let width = frame.width();
        let height = frame.height();

        let ops = &filter.ops;
        for i in 0..width {
            for j in 0..height {
                let pos = Pos(i, j);
                frame
                    .set_pixel(
                        &pos,
                        &crate::media::frame::Color::RGBA(ops.r, ops.g, ops.b, ops.a),
                    )
                    .map_err(|_| PipelineError::PixelError)?;
            }
        }

        Ok(())
    }
}

pub enum PipelineError {
    InvalidData,
    PixelError,
}
