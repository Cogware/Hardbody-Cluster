#![no_std]
#![no_main]
extern crate alloc;
use adafruit_qt_py_rp2040::entry;
use adafruit_qt_py_rp2040::{hal, Pins, XOSC_CRYSTAL_FREQ};
use ads1x1x::{channel, Ads1x1x, DataRate16Bit, TargetAddr};

use nb::block;
use panic_halt as _;
use rp2040_hal::pac::SCB;
use HardbodyCluster::{draw_fuel_gauge, draw_temp_gauge};

use core::cell::RefCell;
use embedded_hal_bus::i2c;

use embedded_alloc::Heap;
use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};
use fugit::RateExtU32;
use hal::{clocks::init_clocks_and_plls, pac, timer::Timer, watchdog::Watchdog, Sio, I2C};
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

#[global_allocator]
static HEAP: Heap = Heap::empty();
#[entry]
fn main() -> ! {
    loop {
        if let Err(_) = run_once() {
            fatal_reset(); // hard reboot
        }
    }
}

fn run_once() -> Result<(), ()> {
    let mut pac = pac::Peripherals::take().unwrap();
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let sio = Sio::new(pac.SIO);
    let mut pins = Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );
    let clocks = init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();
    let mut i2c = I2C::i2c0(
        pac.I2C0,
        pins.sda.reconfigure(), // sda
        pins.scl.reconfigure(), // scl
        100.kHz(),
        &mut pac.RESETS,
        125_000_000.Hz(),
    );

    let i2c_ref_cell = RefCell::new(i2c);

    let interface1 = I2CDisplayInterface::new(i2c::RefCellDevice::new(&i2c_ref_cell));
    let interface2 =
        I2CDisplayInterface::new_alternate_address(i2c::RefCellDevice::new(&i2c_ref_cell));
    let mut adc = Ads1x1x::new_ads1115(i2c::RefCellDevice::new(&i2c_ref_cell), TargetAddr::Gnd);
    adc.set_data_rate(DataRate16Bit::Sps128).unwrap();
    adc.set_full_scale_range(ads1x1x::FullScaleRange::Within4_096V)
        .map_err(|_| ())?;

    let mut display1 = Ssd1306::new(interface1, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display1.init().map_err(|_| ())?;

    let mut display2 = Ssd1306::new(interface2, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display2.init().map_err(|_| ())?;

    let mut _timer = Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);

    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 1024;
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe { HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE) }
    }

    let mut _timer = _timer; // rebind to force a copy of the timer

    display1.clear(BinaryColor::Off).map_err(|_| ())?;
    display2.clear(BinaryColor::Off).map_err(|_| ())?;
    display1.flush().map_err(|_| ())?;
    display2.flush().map_err(|_| ())?;

    loop {
        let mut _discard = block!(adc.read(channel::SingleA0)).map_err(|_| ())?;
        let temp = block!(adc.read(channel::SingleA0)).map_err(|_| ())?;
        _discard = block!(adc.read(channel::SingleA3)).map_err(|_| ())?;
        let calibration1 = block!(adc.read(channel::SingleA3)).map_err(|_| ())?;
        _discard = block!(adc.read(channel::SingleA1)).map_err(|_| ())?;
        let fuel = block!(adc.read(channel::SingleA1)).map_err(|_| ())?;
        _discard = block!(adc.read(channel::SingleA3)).map_err(|_| ())?;
        let calibration2 = block!(adc.read(channel::SingleA3)).map_err(|_| ())?;
        _discard = block!(adc.read(channel::SingleA2)).map_err(|_| ())?;
        let batt_voltage = block!(adc.read(channel::SingleA2)).map_err(|_| ())?;

        display1.clear(BinaryColor::Off).map_err(|_| ())?;
        draw_temp_gauge(&mut display1, temp, calibration1).map_err(|_| ())?;
        display1.flush().map_err(|_| ())?;
        display2.clear(BinaryColor::Off).map_err(|_| ())?;
        draw_fuel_gauge(&mut display2, fuel, batt_voltage, calibration2).map_err(|_| ())?;
        display2.flush().map_err(|_| ())?;
    }
}

#[inline(never)]
fn fatal_reset() -> ! {
    SCB::sys_reset()
}
