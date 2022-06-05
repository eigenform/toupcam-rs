
use toupcam::*;
use std::fs::File;
use std::io::Write;

fn main() -> Result<(), Error> {
    let mut cam = Camera::open()?;
    cam.start_stream()?;

    let mut framebuf = Vec::new();
    let mut fidx = 0;
    'main: loop {
        match cam.read_frame() {
            Ok(frame) => { 
                framebuf.push(frame);
                fidx += 1; 
                println!("got frame {}", fidx);
            },
            Err(toupcam::Error::FirstFrame) => continue,
            Err(e) => {
                println!("{:?}", e);
                break 'main;
            }
        }
        if fidx >= 8 {
            break 'main;
        }
    }
    cam.stop_stream()?;
    println!("stopped streaming");

    for (idx, frame) in framebuf.iter().enumerate() {
        println!("checking frame {}", idx);

        let buf: &[u16] = unsafe {
            std::slice::from_raw_parts(frame.data.as_ptr() as *const u16,
                frame.data.len() / 2)
        };
        let min = buf.iter().min().unwrap();
        let max = buf.iter().max().unwrap();
        let avg: usize = buf.iter().map(|x| *x as usize).sum::<usize>() / buf.len();

        let fname = format!("/tmp/img_{:03}.raw", idx);
        let mut f = File::create(&fname).unwrap();
        f.write(&frame.data).unwrap();
        println!("Wrote {} (min={:04x} max={:04x} avg={:04x})", 
                 fname, min, max, avg as u16);
    }

    Ok(())

}
