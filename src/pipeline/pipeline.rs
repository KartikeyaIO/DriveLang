use crate::filter::{Filter,AudioFilter,AudioContext, FilterVM};
use crate::media::{frame::{Color, Frame, Pos},track::{Track,TrackError}};
use crate::pipeline::kernel::Kernel;
use crate::range::Mask;

pub enum Operation {
    PointFilter {
        filter: Filter,
        params: Vec<f32>,
        mask: Option<Mask>,
    },

    Convolution {
        kernel: Kernel,
        mask: Option<Mask>,
    },

    
    NativeResize {
        width: u32,
        height: u32,
    },
    
    NativeCrop {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
    Blend{
        x:u32,
        y:u32,
        frame2: Frame,
        alpha: f64,
    },
    
}



#[derive(Debug)]
pub enum PipelineError {
    InvalidData,
    PixelError,
    NotFeasible,
}

pub trait Pipeline {
    /// Runs all operations on a frame in a consolidated pass.
    fn execute(&self, frame: &mut Frame) -> Result<(), PipelineError>;
}

pub struct EffectPipeline {
    pub operations: Vec<Operation>,
}

pub struct AudioPipeline {
    pub operations: Vec<AudioOperation>,
}

pub enum AudioOperation {
    PointFilter {
        filter: AudioFilter,
        params: Vec<f32>,
    },
    Gain(f32),
   
}

impl AudioPipeline {
    pub fn execute(&self, track: &mut Track) -> Result<(), PipelineError> {
        let sr = track.sample_rate() as f32;
        let mut vm = FilterVM::new();

        for op in &self.operations {
            match op {
                AudioOperation::PointFilter { filter, params } => {
                    for frame in track.buffer_mut() {
                        let base_time = frame.time.to_seconds() as f32;
                        let samples_len = frame.data.first().map_or(0, |ch| ch.len());
                        let dt = 1.0 / sr;

                        for i in 0..samples_len {
                            let exact_t = base_time + (i as f32 * dt);
                            let l_in = frame.data[0][i];
                            let r_in = if frame.data.len() > 1 { frame.data[1][i] } else { l_in };

                            let (l_out, r_out) = filter.apply(l_in, r_in, exact_t, sr, params, &mut vm);

                            frame.data[0][i] = l_out;
                            if frame.data.len() > 1 {
                                frame.data[1][i] = r_out;
                            }
                        }
                    }
                }
            
                AudioOperation::Gain(db) => track.gain(*db),
            }
        }
        Ok(())
    }
}

impl Pipeline for EffectPipeline {
    fn execute(&self, frame: &mut Frame) -> Result<(), PipelineError> {
        // NOTE: We DO NOT declare width and height out here anymore!
        // The frame size might change mid-pipeline, so we must ask for it on every pass!

        for operation in &self.operations {
            match operation {
                Operation::PointFilter {
                    filter,
                    params,
                    mask,
                } => {
                    let mut vm = FilterVM::new();
                    let width = frame.width();
                    let height = frame.height();

                    for y in 0..height {
                        for x in 0..width {
                            if let Some(mask) = mask {
                                if !mask.contains(x as usize, y as usize) {
                                    continue;
                                }
                            }

                            let pos = Pos(x, y);

                            let color = frame.get_pixel(&pos).unwrap_or(Color::RGB(0, 0, 0));

                            let result = filter.apply(color, x, y, width, height, params, &mut vm);

                            frame
                                .set_pixel(&pos, &result)
                                .map_err(|_| PipelineError::PixelError)?;
                        }
                    }
                }

                Operation::Convolution { kernel, mask } => {
                    let width = frame.width();
                    let height = frame.height();
                    // Snapshot BEFORE this kernel pass
                    let snapshot = frame.clone();

                    for y in 0..height {
                        for x in 0..width {
                            if let Some(mask) = mask {
                                if !mask.contains(x as usize, y as usize) {
                                    continue;
                                }
                            }

                            let pos = Pos(x, y);

                            let result = kernel.apply_to_pixel(x, y, &snapshot);

                            frame
                                .set_pixel(&pos, &result)
                                .map_err(|_| PipelineError::PixelError)?;
                        }
                    }
                }
                Operation::Blend {x,y, frame2, alpha } =>{
                    frame.blend_on(&Pos(*x, *y),frame2, *alpha).map_err(|_| PipelineError::NotFeasible)?;
                    
                }

                Operation::NativeResize { width, height } => {
                    
                    let new_frame = frame.resize(*width, *height).map_err(|_| PipelineError::NotFeasible)?;
                    
                    
                    *frame = new_frame; 
                }

                Operation::NativeCrop { x, y, width, height } => {
                    let new_frame = frame.crop(*x, *y, *width, *height).map_err(|_| PipelineError::NotFeasible)?;
                    *frame = new_frame;
                } 
                
            }
        }

        Ok(())
    }
}