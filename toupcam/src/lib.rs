
use std::fs::File;
use std::io::Write;
use std::time::Duration;

use pretty_hex::*;
use rusb::{
    Context, UsbContext, 
    Device, DeviceHandle, DeviceDescriptor, 
    Direction, request_type, Recipient, RequestType,
};

/// Wrapper for [rusb::Error]
#[derive(Debug)]
pub enum Error { 
    Rusb(rusb::Error),
    FirstFrame,
}
impl From<rusb::Error> for Error {
    fn from(e: rusb::Error) -> Self { Self::Rusb(e) }
}

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

/// Representing a camera device.
pub struct Camera {
    ctx: Context,
    pub dev: Device<Context>,
    pub desc: DeviceDescriptor,
    pub handle: DeviceHandle<Context>,
    timeout: Duration,
    streaming: bool,
}
impl Camera {
    /// Magic constant for [de]obfuscating some vendor requests
    const XOR_VAL: u16 = 0x0000;

    /// Open the camera
    pub fn open(vid: u16, pid: u16) -> Result<Self, Error> {
        let timeout = Duration::from_secs(5);
        let mut ctx = Context::new().unwrap();
        let mut res = match open_device(&mut ctx, vid, pid) {
            Ok((mut dev, desc, mut handle)) => { 
                Self { 
                    ctx, dev, desc, handle, timeout,
                    streaming: false,
                }
            },
            Err(e) => return Err(Error::Rusb(e)),
        };

        match res.handle.kernel_driver_active(0) {
            Ok(true) => {
                println!("kernel driver active?");
                res.handle.detach_kernel_driver(0)?;
                println!("detached kernel driver");
            },
            _ => {
                println!("kernel driver not active?");
            }
        }

        //res.handle.reset()?;
        res.handle.set_active_configuration(1)?;
        res.handle.claim_interface(0)?;

        Ok(res)
    }

    /// Walk and print descriptors for the device
    pub fn walk_config(&self) -> Result<(), Error> {
        for n in 0..self.desc.num_configurations() {
            let cdesc = self.dev.config_descriptor(n)?;
            for iface in cdesc.interfaces() {
                for idesc in iface.descriptors() {
                    for ep in idesc.endpoint_descriptors() {
                        println!("if={:02x} setting={:02x} addr={:02x}", 
                                 idesc.interface_number(),
                                 idesc.setting_number(),
                                 ep.address()
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

impl Camera {

    /// Special writes to [what I assume are the] sensor registers.
    fn sensor_write(&mut self, addr: u16, val: u16) -> Result<(), Error> {
        let mut buf: [u8; 1] = [ 0 ];
        let rt = request_type(Direction::In, RequestType::Vendor, Recipient::Device);

        // Seems like these write to 0x1100 on success?
        self.handle.read_control(rt, 0x0b, val, addr, &mut buf, self.timeout)?;
        if buf[0] == 0x08 {
            self.handle.read_control(rt, 0x0b, val, 0x1100, &mut buf, self.timeout)?;
        } else {
            println!("sensor write to {:04x} returned {:02x}?", addr, buf[0]);
        }
        Ok(())
    }

    // Write requests? 
    fn sys_write(&mut self, addr: u16, val: u16) -> Result<(), Error> {
        let mut buf: [u8; 1] = [ 0 ];
        let rt = request_type(Direction::In, RequestType::Vendor, Recipient::Device);
        match self.handle.read_control(rt, 0x0b, val, addr, &mut buf, self.timeout) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::from(e)),
        }
    }

    // Read requests? Usually issued in pairs? 
    fn sys_read(&mut self, addr: u16) -> Result<(u16), Error> {
        let mut buf: [u8; 2] = [0, 0];
        self.ven_in(0x0a, 0x0000, addr, &mut buf)?;
        Ok(u16::from_le_bytes(buf))
    }

    /// Send a vendor command (input).
    fn ven_in(&mut self, req: u8, val: u16, idx: u16, buf: &mut [u8]) 
        -> Result<(), Error> 
    {
        let rt = request_type(Direction::In, RequestType::Vendor, Recipient::Device);
        match self.handle.read_control(rt, req, val, idx, buf, self.timeout) {
            Ok(_) => { Ok(()) },
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Send a vendor command (output).
    fn ven_out(&mut self, req: u8, val: u16, idx: u16, buf: &[u8]) 
        -> Result<(), Error> 
    {
        let rt = request_type(Direction::Out, RequestType::Vendor, Recipient::Device);
        match self.handle.write_control(rt, req, val, idx, buf, self.timeout) {
            Ok(_) => { Ok(()) },
            Err(e) => Err(Error::from(e)),
        }
    }

    // Set exposure parameters?
    //
    // It seems like `0x1064` and `0x5000` are the only ones that vary.
    // Not clear how this works yet.
    fn set_exposure(&mut self, val1064: u16, val5000: u16) -> Result<(), Error> {
        self.sensor_write(0x1063, 0x0000)?;
        self.sensor_write(0x1064, val1064)?;
        self.sys_write(0x4000, 0x0000)?;
        self.sys_write(0x5000, val5000)?;
        Ok(())
    }

    /// Initial configuration for the sensor (here be dragons).
    ///
    /// Mostly replicated from USB packet captures: this sequence is not
    /// well understood, and may not be generalizable to different initial
    /// configurations of the camera.
    ///
    /// This corresponds [AFAIK] to the following initial setup:
    ///
    /// 1. Set size to mode 1
    /// 2. Set TOUPCAM_OPTION_RAW to 1
    /// 3. Set TOUPCAM_OPTION_BITDEPTH to 1
    /// 4. Set auto-exposure enable to false
    ///
    /// It's also not clear whether or not some settings are automatically
    /// saved/restored via EEPROM state without needing configuration here.
    ///
    /// # Safety
    /// The probability of damaging the sensor here is non-zero!
    /// It doesn't help that I have no clue how this *really* works.
    ///
    /// Furthermore, it also seems particular commands in this sequence are 
    /// sensitive to timing. Tolerance [and generally all of this] is unclear.
    ///
    fn sensor_init(&mut self) -> Result<(), Error> {

        self.sys_write(0x0200, 0x0001)?;
        self.sys_write(0x8000, 0x09b0)?;
        self.set_exposure(0x0637, 0x0e24)?;

        // -------
        self.sensor_write(0x1008, 0x4299)?; 
        self.sensor_write(0x100f, 0x7fff)?; 
        self.sensor_write(0x1001, 0x0030)?; 
        self.sensor_write(0x1002, 0x0003)?;
        self.sensor_write(0x1003, 0x07e9)?; 
        self.sensor_write(0x1000, 0x0003)?; 
        self.sensor_write(0x1004, 0x0087)?;  // related to mode 0?
        self.sensor_write(0x1006, 0x1104)?;  // related to mode 0?
        self.sensor_write(0x1009, 0x02c0)?; 
        self.sensor_write(0x1005, 0x0001)?; 
        self.sensor_write(0x1007, 0x7fff)?; 
        self.sensor_write(0x100a, 0x0000)?;
        self.sensor_write(0x100b, 0x0100)?; 
        self.sensor_write(0x100c, 0x0000)?; 
        self.sensor_write(0x100d, 0x2090)?; 
        self.sensor_write(0x100e, 0x0103)?;
        self.sensor_write(0x1010, 0x0000)?; 
        self.sensor_write(0x1011, 0x0000)?; 
        std::thread::sleep(Duration::from_millis(5));
        self.sensor_write(0x1000, 0x0053)?; 
        self.sensor_write(0x1008, 0x0298)?;
        std::thread::sleep(Duration::from_millis(5));

        // -------
        self.sys_write(0x1200, 0x0001)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?
        self.sys_write(0x2000, 0x0000)?;
        self.sys_write(0x1200, 0x0002)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?
        self.sys_write(0x0200, 0x0001)?;
        self.sys_write(0x0a00, 0x0001)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?
        self.sys_write(0x0a00, 0x0000)?;
        std::thread::sleep(Duration::from_millis(20)); // should be 20?

        // -------
        self.sensor_write(0x1008, 0x4299)?; 
        self.sensor_write(0x100f, 0x7fff)?; 
        self.sensor_write(0x1001, 0x0030)?; 
        self.sensor_write(0x1002, 0x0003)?;
        self.sensor_write(0x1003, 0x07e9)?; 
        self.sensor_write(0x1000, 0x0003)?; 
        self.sensor_write(0x1004, 0x0083)?; // related to mode 1/2?
        self.sensor_write(0x1006, 0x11dc)?; // related to mode 1/2?
        self.sensor_write(0x1009, 0x02c0)?; 
        self.sensor_write(0x1005, 0x0001)?; 
        self.sensor_write(0x1007, 0x7fff)?; 
        self.sensor_write(0x100a, 0x0000)?;
        self.sensor_write(0x100b, 0x0100)?; 
        self.sensor_write(0x100c, 0x0000)?; 
        self.sensor_write(0x100d, 0x2090)?; 
        self.sensor_write(0x100e, 0x0103)?;
        self.sensor_write(0x1010, 0x0000)?; 
        self.sensor_write(0x1011, 0x0000)?; 
        std::thread::sleep(Duration::from_millis(5));
        self.sensor_write(0x1000, 0x0053)?; 
        self.sensor_write(0x1008, 0x0298)?;
        std::thread::sleep(Duration::from_millis(5));

        // -------
        self.sys_write(0x103b, 0x0000)?;

        self.sys_write(0x2000, 0x0001)?; // related to mode 1
        self.sys_write(0x1200, 0x0003)?; // related to mode 1
        std::thread::sleep(Duration::from_millis(10));

        // Perhaps resolution related?
        self.sys_write(0x8000, 0x060c)?; // related to mode 1?

        self.set_exposure(0x000a, 0x0cbd)?;

        self.sys_write(0x0a00, 0x0001)?;
        //std::thread::sleep(Duration::from_millis(10));

        self.set_exposure(0x000a, 0x0cbd)?;

        // This is definitely the register for analog gain
        self.sensor_write(0x1061, 0x610c)?;

        Ok(())
    }

    /// Read from EEPROM?
    fn read_eeprom(&mut self) -> Result<(), Error> {
        let mut eeprom_buf_1: [u8; 0x1000] = [0; 0x1000];
        let mut eeprom_buf_2: [u8; 0x0cbb] = [0; 0x0cbb];
        self.ven_in(0x20, 0x0000, 0x0000, &mut eeprom_buf_1)?;
        self.ven_in(0x20, 0x1000, 0x0000, &mut eeprom_buf_2)?;

        use crypto::sha1::*;
        use crypto::digest::*;
        let mut d = Sha1::new();
        d.input(&eeprom_buf_1);
        d.input(&eeprom_buf_2);
        let hex = d.result_str();
        println!("EEPROM SHA1 digest: {}", hex);
        Ok(())
    }

    /// Configure the device and start streaming data
    pub fn start_stream(&mut self) -> Result<(), Error> {
        if self.streaming {
            return Ok(())
        }

        let mut hbuf: [u8; 2] = [0; 2];

        // No idea
        self.ven_in(0x16, Self::XOR_VAL, 0x0000, &mut hbuf)?;

        // Some kind of other initialization command.
        // Best guess is: maybe related to enabling sensor configuration?
        self.ven_out(0x01, 0x0001, 0x000f, &[])?;
        //self.ven_out(0x01, 0x0000, 0x000f, &[])?;
        //self.ven_out(0x01, 0x0001, 0x000f, &[])?;

        // These seem like read accesses. No clue.
        self.ven_in(0x0a, 0x0000, 0xffff, &mut hbuf)?;
        self.ven_in(0x0a, 0x0000, 0xffff, &mut hbuf)?;
        self.ven_in(0x0a, 0x0000, 0xfeff, &mut hbuf)?;
        self.ven_in(0x0a, 0x0000, 0xfeff, &mut hbuf)?;

        // Read from EEPROM?
        // No idea how it's used during initialization, if at all.
        //self.read_eeprom()?;

        // Write the initial sensor configuration
        self.sensor_init()?;

        // Data should be available for bulk transfer after this command
        self.ven_out(0x01, 0x0003, 0x000f, &[])?;
        std::thread::sleep(Duration::from_millis(10));

        self.streaming = true;
        Ok(())
    }

    /// Stop streaming data.
    pub fn stop_stream(&mut self) -> Result<(), Error> {
        if !self.streaming {
            Ok(())
        } else {
            let mut wbuf: [u8; 4] = [0; 4];

            self.sys_write(0x0a00, 0x0000)?;
            self.sensor_write(0x1000, 0x0000)?;
            self.ven_out(0x01, 0x0000, 0x000f, &[])?;
            let res = self.ven_in(0x17, 0x0000, 0x0000, &mut wbuf);

            std::thread::sleep(Duration::from_millis(10));
            self.streaming = false;
            println!("stopped stream");
            res
        }
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        match self.stop_stream() {
            Ok(_) => {},
            Err(e) => println!("Couldn't stop streaming? {:?}", e),
        }

        match self.handle.release_interface(0) {
            Ok(_) => {},
            Err(e) => println!("Couldn't release interface 0? {}", e),
        }
    }
}


impl Camera {

    /// Read a "frame" from the device. 
    pub fn read_frame(&mut self) -> Result<Vec<u8>, Error> {
        let mut to    = Duration::from_millis(500);

        // This seems like the maximum transfer size on my machine.
        const CHUNK_LEN: usize  = 0x0004_0000;
        let mut buf   = [0u8; CHUNK_LEN];

        // NOTE: I'm pretty sure the initial sensor configuration is tied
        // to this mode right now
        const HEIGHT: usize = 2320;
        const WIDTH: usize  = 1740;
        let frame_len = ((HEIGHT * WIDTH) * 2);

        let mut frame = vec![0u8; frame_len];
        let mut cur   = 0;

        // NOTE: I think libusb turns these into DMA - if you're looking at 
        // packets from this, there won't be any response data in the URBs
        loop {
            match self.handle.read_bulk(0x81, &mut buf, to) {
                Ok(rlen) => {
                    // If the incoming data would overflow the buffer,
                    // just truncate it and copy the remaining bytes
                    let rem = frame_len - cur;
                    let len = if rlen > rem { rem } else { rlen };

                    // Copy into frame buffer
                    frame[cur..cur+len].copy_from_slice(&buf[..len]);
                    cur += len;

                    // If we get less bytes than we requested, this indicates
                    // that the device has finished reading out a frame.
                    if rlen < CHUNK_LEN {
                        break;
                    }

                },
                Err(e) => return Err(Error::from(e)),
            }
        }

        // The first frame after initialization is typically truncated.
        // Just return an error so we can discard it.
        if cur < frame_len {
            Err(Error::FirstFrame)
        } else {
            Ok(frame)
        }
    }
}

/// A raw frame from the sensor.
pub struct Frame { 
    pub height: usize,
    pub width: usize,
    pub raw16: bool,
    pub size: usize,
    pub data: Vec<u8>,
}
impl Frame {
    pub fn new(h: usize, w: usize, raw16: bool) -> Self {
        let sz = if raw16 { (h * w) * 2 } else { h * w };
        Self { 
            height: h, width: w, raw16, size: sz, data: vec![0u8; sz] }
    }
}

