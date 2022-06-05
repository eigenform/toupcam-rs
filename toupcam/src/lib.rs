
mod usb;
mod sensor;

use std::time::Duration;
use rusb::{ Context, UsbContext, Device, DeviceHandle, DeviceDescriptor };

/// Bit depth of raw sensor data
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BitDepth { BitDepth8, BitDepth12 }

/// Supported sensor/readout resolution.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CameraMode { Mode0, Mode1, Mode2 }
impl CameraMode {
    pub fn dimensions(self) -> (usize, usize) {
        match self {
            Self::Mode0 => (4632, 3488),
            Self::Mode1 => (2320, 1740),
            Self::Mode2 => (1536, 1160),
        }
    }
}

/// Wrapper for [rusb::Error]
#[derive(Debug)]
pub enum Error { 
    Rusb(rusb::Error),
    FirstFrame,
    Unimplemented,
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
    /// libusb context associated with this device
    _ctx: Context,

    /// libusb object associated with this USB device
    _dev: Device<Context>,

    /// Descriptor for this USB device
    _desc: DeviceDescriptor,

    /// libusb handle for this USB device
    handle: DeviceHandle<Context>,

    /// Default timeout for commands
    timeout: Duration,

    /// Set to 'true' when the camera is streaming data.
    streaming: bool,
    /// The current sensor/readout mode.
    mode: CameraMode,
    /// The current bit-depth.
    depth: BitDepth,

}
impl Camera {
    /// Open an instance of the camera.
    ///
    /// This assumes the VID/PID for the device is `0x0547:0x3016`.
    pub fn open() -> Result<Self, Error> {
        const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
        const DEFAULT_MODE: CameraMode  = CameraMode::Mode1;
        const DEFAULT_DEPTH: BitDepth   = BitDepth::BitDepth12;
        const VID: u16 = 0x0547;
        const PID: u16 = 0x3016;

        let mut _ctx = Context::new().unwrap();
        let mut res = match open_device(&mut _ctx, VID, PID) {
            Ok((_dev, _desc, handle)) => { 
                Self { _ctx, _dev, _desc, handle, 
                    timeout: DEFAULT_TIMEOUT, 
                    mode: DEFAULT_MODE,
                    depth: DEFAULT_DEPTH,
                    streaming: false,
                }
            },
            Err(e) => return Err(Error::Rusb(e)),
        };

        if let Ok(true) = res.handle.kernel_driver_active(0) {
            res.handle.detach_kernel_driver(0)?;
        }
        res.handle.set_active_configuration(1)?;
        res.handle.claim_interface(0)?;

        Ok(res)
    }

    pub fn get_mode(&self) -> CameraMode { self.mode }
    pub fn get_depth(&self) -> BitDepth { self.depth }
    pub fn set_depth(&mut self, depth: BitDepth) -> Result<(), Error> {
        if depth == self.depth { return Ok(()) }
        if self.streaming { return Err(Error::Unimplemented) }
        self.depth = depth;
        Ok(())
    }
    pub fn set_mode(&mut self, mode: CameraMode) -> Result<(), Error> {
        if mode == self.mode { return Ok(()); }
        if self.streaming { return Err(Error::Unimplemented); }
        self.mode = mode;
        Ok(())
    }

    /// Configure the device and start streaming data
    pub fn start_stream(&mut self) -> Result<(), Error> {
        if self.streaming { return Ok(()) }

        // Set the magic XOR value to zero
        let mut hbuf: [u8; 2] = [0; 2];
        self.ven_in(0x16, 0x0000, 0x0000, &mut hbuf)?;

        self.ven_out(0x01, 0x0001, 0x000f, &[])?;
        //self.ven_out(0x01, 0x0000, 0x000f, &[])?;
        //self.ven_out(0x01, 0x0001, 0x000f, &[])?;

        self.ven_in(0x0a, 0x0000, 0xffff, &mut hbuf)?;
        self.ven_in(0x0a, 0x0000, 0xffff, &mut hbuf)?;
        self.ven_in(0x0a, 0x0000, 0xfeff, &mut hbuf)?;
        self.ven_in(0x0a, 0x0000, 0xfeff, &mut hbuf)?;

        self.sensor_init()?;

        // After this command, frames should be available for us to read with
        // bulk transfers on endpoint 0x81.
        self.ven_out(0x01, 0x0003, 0x000f, &[])?;
        std::thread::sleep(Duration::from_millis(10));

        self.streaming = true;
        Ok(())
    }

    /// Stop streaming data.
    ///
    /// Presumably this also clears the sensor configuration.
    pub fn stop_stream(&mut self) -> Result<(), Error> {
        if !self.streaming { return Ok(()); }

        self.sys_write(0x0a00, 0x0000)?;
        self.sensor_write(0x1000, 0x0000)?;
        self.ven_out(0x01, 0x0000, 0x000f, &[])?;

        let mut wbuf: [u8; 4] = [0; 4];
        self.ven_in(0x17, 0x0000, 0x0000, &mut wbuf)?;
        std::thread::sleep(Duration::from_millis(10));

        self.streaming = false;
        Ok(())
    }
}



/// Container for a frame of raw image data returned by the device.
pub struct Frame {
    /// Raw image data (in bytes)
    pub data: Vec<u8>,
    /// Number of rows
    pub height: usize,
    /// Number of columns
    pub width: usize,
    /// Number of bytes per pixel
    pub bpp: usize,
    pub elapsed: std::time::Duration,
}

impl Camera {
    /// Try to read out an entire frame from the device. 
    pub fn read_frame(&mut self) -> Result<Frame, Error> {
        let timeout = Duration::from_millis(500);

        // This seems like the maximum transfer size on my machine.
        const CHUNK_LEN: usize  = 0x0004_0000;
        let mut buf   = [0u8; CHUNK_LEN];

        // Allocate space to hold a completed frame
        let (width, height) = self.mode.dimensions();
        let bpp = match self.depth {
            BitDepth::BitDepth12 => 2,
            BitDepth::BitDepth8  => 1,
        };
        let frame_len = (width * height) * bpp;
        let mut data = vec![0u8; frame_len];
        let mut cur  = 0;

        // Issue bulk reads until we've received an entire frame
        let start = std::time::Instant::now();
        loop {
            match self.handle.read_bulk(0x81, &mut buf, timeout) {
                Ok(rlen) => {
                    // If the incoming data would overflow the buffer,
                    // just truncate it and copy the remaining bytes
                    let rem = frame_len - cur;
                    let len = if rlen > rem { rem } else { rlen };

                    // Copy into frame buffer
                    data[cur..cur+len].copy_from_slice(&buf[..len]);
                    cur += len;

                    // If we get less bytes than we requested, this indicates
                    // that the device has finished reading out a frame.
                    if rlen < CHUNK_LEN { break; }
                },
                Err(e) => return Err(Error::from(e)),
            }
        }
        let elapsed = start.elapsed();

        // This really only occurs on the first frame after initialization; 
        // the data is typically truncated, and we can just discard it.
        if cur < frame_len {
            Err(Error::FirstFrame)
        } else {
            Ok(Frame { width, height, bpp, data, elapsed })
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
        match self.handle.reset() {
            Ok(_) => {},
            Err(e) => println!("Couldn't reset handle? {}", e),
        }
    }
}

