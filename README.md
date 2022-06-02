# toupcam-rs

Hacky library for interfacing with the AmScope MU1603 (Touptek U3CMOS16000KPA)
USB3.0 CMOS camera.

Don't exactly know what I'm doing with this yet. There are no safety guarantees
here, and it's not clear if you can damage the device by using this (although
my guess is probably **yes**). Proceed at your own risk.

- `toupcam/` - Library crate
- `toupcam-ui/` - Simple UI for live capture with SDL2 
- `utils/` - Miscellania

## About

The VID/PID for this model is `0x0547:0x3016`.
I think the VID implies a Cypress USB microcontroller (perhaps the FX3)?

Touptek product sheets say the sensor is a [Panasonic] MN34120, but it doesn't
seem like there are any useful Panasonic datasheets for this part.

## Prior Art

See John McMaster's work on the MU800:

- [JohnDMcMaster/uscope-cam-wip](https://github.com/JohnDMcMaster/uscope-cam-wip)
- [drivers/media/usb/gspca/touptek.c](https://github.com/torvalds/linux/blob/master/drivers/media/usb/gspca/touptek.c)

