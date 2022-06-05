
use sdl2::pixels::PixelFormatEnum;
use bayer::{ RasterMut, RasterDepth };

use std::sync::mpsc::*;
use std::fs::File;
use std::io::Read;
use std::time::{ Instant, Duration };

enum CameraCtrl {
    Stop
}

struct DataPacket { frame: toupcam::Frame, ts: Instant }


fn main() {

    // Channel for moving data from the camera thread to the main thread
    let (frame_tx, frame_rx) = channel();
    let (ctrl_tx, ctrl_rx) = channel();

    // Brief SDL2 setup.
    // All we need is a way to draw RGB24 textures.
    let sdl    = sdl2::init().unwrap();
    let video  = sdl.video().unwrap();
    let window = video.window("Preview", 2320, 1740)
        .position_centered().opengl().build().unwrap();
    let mut canvas = window.into_canvas().build().unwrap();
    let mut event_pump = sdl.event_pump().unwrap();
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator.create_texture_streaming(
        PixelFormatEnum::RGB24, 2320, 1740
    ).unwrap();


    // Spawn the camera thread.
    // Presumably the channel will buffer up pointers to frames for us.
    let handle = std::thread::spawn(move || -> Result<(), toupcam::Error> {
        let mut cam = toupcam::Camera::open()?;
        cam.start_stream()?;
        let mut fidx = 0;
        'main: loop {
            match cam.read_frame() {
                Ok(frame) => { 
                    let pkt = DataPacket { frame, ts: Instant::now() };
                    fidx += 1; 
                    frame_tx.send(pkt).unwrap();
                    println!("sent frame {}", fidx);
                },
                Err(toupcam::Error::FirstFrame) => { 
                    println!("skipped first frame");
                    continue; 
                },
                Err(e) => {
                    println!("{:?}", e);
                    break 'main;
                }
            }
            match ctrl_rx.try_recv() {
                Err(TryRecvError::Empty) => {},
                Ok(msg) => {
                    println!("shutting down camera thread");
                    break 'main;
                },
                Err(TryRecvError::Disconnected) => {
                    println!("camera control disconnected?");
                    break 'main;
                },
            }
        }
        cam.stop_stream()?;
        println!("camera thread finished");
        Ok(())
    });

    // Allocation for the raster object.
    // All of these pixels are recomputed each time we demosaic a frame
    let mut rasbuf = vec![0; 6 * (2320 * 1740)];

    let mut connected = true;
    let mut redraw = true;
    'main: loop {

        // If the camera thread is connected, try to read and process a frame
        if connected {
            match frame_rx.try_recv() {
                Ok(pkt) => {
                    println!("got {}", pkt.frame.data.len());
                    let recv_ts = std::time::Instant::now();
                    let recv_elapsed = pkt.ts.elapsed();

                    // Demosaic the raw frame
                    let mut ras = RasterMut::new(2320, 1740, 
                        RasterDepth::Depth16, &mut rasbuf);
                    bayer::run_demosaic(&mut pkt.frame.data.as_slice(), 
                        bayer::BayerDepth::Depth16BE, bayer::CFA::RGGB, 
                        bayer::Demosaic::Linear, &mut ras
                    );

                    let buf: &[u16] = unsafe { std::slice::from_raw_parts(
                        rasbuf.as_ptr() as *const u16, rasbuf.len() / 2)
                    };

                    // Update the texture
                    texture.with_lock(None, |buffer: &mut [u8], pitch: usize| {
                        for y in 0..1740 {
                            let src_offset = (3 * 2320) * y;
                            let dst_offset = pitch * y;
                            for i in 0..3 * 2320 {
                                let v = buf[src_offset + i] >> 8;
                                buffer[dst_offset + i] = std::cmp::min(v, 255) as u8;
                            }
                        }
                    }).unwrap();
                    let upd_elapsed = recv_ts.elapsed();
                    redraw = true;

                    println!("frame read={:?} recv={:?} upd={:?}", 
                            pkt.frame.elapsed, recv_elapsed, upd_elapsed);
                },
                Err(TryRecvError::Empty) => {},
                Err(TryRecvError::Disconnected) => {
                    println!("camera thread disconnected");
                    connected = false;
                    redraw = false;
                },
            }
        }

        if redraw {
            // Redraw the canvas
            canvas.clear();
            let _ = canvas.copy(&texture, None, None);
            canvas.present();
            redraw = false;
        }

        // Catch an SDL2 event (i.e. closing the window).
        if let Some(e) = event_pump.wait_event_timeout(1) {
            match e {
                sdl2::event::Event::Quit { .. } => {
                    println!("sent stop message to camera thread");
                    ctrl_tx.send(CameraCtrl::Stop).unwrap();
                    break 'main;
                },
                _ => (),
            }
        }

    }

    // Wait for the camera thread to close
    handle.join().unwrap();
    println!("camera thread all done, seeya!");

}
