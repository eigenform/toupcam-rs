
use std::fs::File;
use std::io::Write;

use std::time::Duration;
use rusb::{
    Context, UsbContext, 
    Device, DeviceHandle, DeviceDescriptor, 
    Direction, request_type, Recipient, RequestType,
};

use pretty_hex::*;


/// Magic constant to avoid obfuscating some control transfers.
const XOR_VAL: u16 = 0x0000;


/// Open a particular device by VID/PID.
fn open_device<T: UsbContext>(ctx: &mut T, vid: u16, pid: u16) 
    -> rusb::Result<(Device<T>, DeviceDescriptor, DeviceHandle<T>)> {
    let devices = ctx.devices()?;
    for device in devices.iter() {
        let desc = device.device_descriptor()?;
        if desc.vendor_id() == vid && desc.product_id() == pid {
            match device.open() {
                Ok(handle) => return Ok((device, desc, handle)),
                Err(e) => return Err(e),
            }
        }
    }
    Err(rusb::Error::NoDevice)
}

#[derive(Debug)]
enum Error {
    /// Errors from rusb
    Rusb(rusb::Error),
}
impl From<rusb::Error> for Error {
    fn from(e: rusb::Error) -> Self { Self::Rusb(e) }
}

struct Camera {
    ctx: Context,
    pub dev: Device<Context>,
    pub desc: DeviceDescriptor,
    pub handle: DeviceHandle<Context>,
    timeout: Duration,
}

/// Interface for a [Camera].
impl Camera {
    pub fn open(vid: u16, pid: u16) -> Result<Self, Error> {
        let timeout = Duration::from_secs(5);
        let mut ctx = Context::new().unwrap();
        let mut res = match open_device(&mut ctx, 0x0547, 0x3016) {
            Ok((mut dev, desc, mut handle)) => { 
                Self { ctx, dev, desc, handle, timeout }
            },
            Err(e) => return Err(Error::Rusb(e)),
        };

        // NOTE: This might not be necessary? Following rusb boilerplate.
        res.handle.reset()?;

        res.handle.claim_interface(0)?;
        res.init()?;

        Ok(res)
    }
}

impl Camera {

    fn ven_in(&mut self, req: u8, val: u16, idx: u16, buf: &mut [u8]) 
        -> Result<(), Error> 
    {
        let rt = request_type(Direction::In, RequestType::Vendor, Recipient::Device);
        //let start = std::time::Instant::now();
        match self.handle.read_control(rt, req, val, idx, buf, self.timeout) {
            Ok(_) => {
                //let diff = start.elapsed();
                //println!("[*] ven_in(0x{:02x}, 0x{:04x}, 0x{:04x}) took {:?}",
                //    req, val, idx, diff);
                Ok(())
            },
            Err(e) => Err(Error::from(e)),
        }
    }

    fn ven_out(&mut self, req: u8, val: u16, idx: u16, buf: &[u8]) 
        -> Result<(), Error> 
    {
        let rt = request_type(Direction::Out, RequestType::Vendor, Recipient::Device);
        //let start = std::time::Instant::now();
        match self.handle.write_control(rt, req, val, idx, buf, self.timeout) {
            Ok(_) => {
                //let diff = start.elapsed();
                //println!("[*] ven_out(0x{:02x}, 0x{:04x}, 0x{:04x}) took {:?}",
                //    req, val, idx, diff);
                Ok(())
            },
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Control sequence for initializing the camera.
    fn init(&mut self) -> Result<(), Error> {
        // Response buffers.
        let mut hbuf: [u8; 2] = [0; 2];
        let mut wbuf: [u8; 4] = [0; 4];

        // 1. Either lazy obfuscation, or maybe for error detection? 
        self.ven_in(0x16, XOR_VAL, 0x0000, &mut hbuf)?;

        // 2. This probably enables programming with 0x0a/0x0b/0x20/etc?
        self.ven_out(0x01, 0x0001, 0x000f, &[])?;


        // The following are probably used by software for validating things.
        // Not functionally necessary?

        // Reads to some address space on the microcontroller?
        //self.ven_in(0x0a, 0x0000, 0xffff, &mut hbuf)?;
        //self.ven_in(0x0a, 0x0000, 0xffff, &mut hbuf)?;
        //self.ven_in(0x0a, 0x0000, 0xfeff, &mut hbuf)?;
        //self.ven_in(0x0a, 0x0000, 0xfeff, &mut hbuf)?;

        // I think this is some kind of validation thing.
        //self.ven_in(0x20, 0x0000, 0x0000, &mut wbuf)?;
        //assert!(wbuf == [0x9b, 0x1c, 0x00, 0x00 ]);

        // NOTE: These are EEPROM reads (the EEPROM is 0x2000 bytes?).
        //let mut eeprom_buf_1: [u8; 0x1000] = [0; 0x1000];
        //let mut eeprom_buf_2: [u8; 0x0cbb] = [0; 0x0cbb];
        //self.ven_in(0x20, 0x0000, 0x0000, &mut eeprom_buf_1)?;
        //self.ven_in(0x20, 0x1000, 0x0000, &mut eeprom_buf_2)?;

        // 3. Seems like this enables bulk transfers for streaming data?
        // NOTE: You might want to defer this until later?
        self.ven_out(0x01, 0x0003, 0x000f, &[])?;

        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), Error> {
        let mut bbuf: [u8; 1] = [0];
        let mut wbuf: [u8; 4] = [0; 4];

        // NOTE: I *think* these only happen when closing the camera.
        // Might be clearing something?
        self.ven_in(0x0b, 0x0000, 0x0a00, &mut bbuf)?;
        assert!(bbuf == [0x08]);
        self.ven_in(0x0b, 0x0000, 0x1000, &mut bbuf)?;
        assert!(bbuf == [0x08]);
        self.ven_in(0x0b, 0x0000, 0x1100, &mut bbuf)?;
        assert!(bbuf == [0x08]);

        // NOTE: I *think* these close/stop/reset the camera.
        self.ven_out(0x01, 0x0000, 0x000f, &[])?;
        self.ven_in(0x17, 0x0000, 0x0000, &mut wbuf)?;
        Ok(())
    }
}

// I *assume* the rest of the rusb resources are dropped too.
impl Drop for Camera {
    fn drop(&mut self) {
        match self.cleanup() {
            Ok(()) => {},
            Err(e) => {
                println!("[!] Cleanup control sequence failed:");
                println!("    {:?}", e);
            },
        }
    }
}

fn main() -> Result<(), Error> {

    let mut cam = Camera::open(0x0547, 0x3016)?;

    std::thread::sleep(Duration::from_secs(5));

    Ok(())
}



