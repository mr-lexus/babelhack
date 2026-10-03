// CoreAudio aggregate dictionary construction is adapted from CPAL 0.18.1,
// Copyright CPAL contributors, Apache-2.0. See licenses/CPAL-APACHE-2.0.txt.
// Changes: force a process tap for every output, including duplex USB/Bluetooth
// devices; own both handles on all error paths; resolve the aggregate by stable UID.
// This avoids CPAL's input-capable-device path, which would record the microphone.
use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait};
use objc2::{rc::Retained, AnyThread};
use objc2_core_audio::{
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
    AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap, CATapDescription,
    CATapMuteBehavior,
};
use objc2_core_foundation::{
    kCFAllocatorDefault, kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks, CFArray, CFDictionary, CFMutableDictionary, CFRetained,
    CFString,
};
use objc2_foundation::{NSArray, NSNumber, NSString};
use std::{
    ffi::{c_void, CStr},
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TAP: AtomicU64 = AtomicU64::new(0);

pub struct Tap {
    tap_id: u32,
    aggregate_id: u32,
}

impl Drop for Tap {
    fn drop(&mut self) {
        // SAFETY: nonzero handles are owned exclusively by this guard. The audio
        // stream is dropped before the guard by AudioCapture's field order.
        unsafe {
            if self.aggregate_id != 0 {
                AudioHardwareDestroyAggregateDevice(self.aggregate_id);
            }
            if self.tap_id != 0 {
                AudioHardwareDestroyProcessTap(self.tap_id);
            }
        }
    }
}

pub fn create(host: &cpal::Host, output: &cpal::Device) -> anyhow::Result<(cpal::Device, Tap)> {
    let uid = output.id()?;
    let excluded_processes = NSArray::new();
    let device_uid = NSString::from_str(uid.id());
    // SAFETY: objects are retained for every call; the selected output UID is
    // provided by CoreAudio. No input device or microphone is opened.
    let description = unsafe {
        CATapDescription::initWithProcesses_andDeviceUID_withStream(
            CATapDescription::alloc(),
            &excluded_processes,
            &device_uid,
            0,
        )
    };
    unsafe {
        description.setExclusive(true);
        description.setPrivate(true);
        description.setMuteBehavior(CATapMuteBehavior::Unmuted);
        description.setName(&NSString::from_str("Babel Hack system audio"));
    }
    let mut guard = Tap {
        tap_id: 0,
        aggregate_id: 0,
    };
    let status = unsafe { AudioHardwareCreateProcessTap(Some(&description), &mut guard.tap_id) };
    if status != 0 {
        return Err(anyhow!("CoreAudio process tap: OSStatus {status}"));
    }
    let tap_uid = unsafe { description.UUID().UUIDString() };
    let aggregate_uid = format!(
        "io.github.mr-lexus.babelhack.tap.{}.{}",
        std::process::id(),
        NEXT_TAP.fetch_add(1, Ordering::Relaxed)
    );
    let properties =
        create_audio_aggregate_device_properties(tap_uid, &aggregate_uid, "Babel Hack loopback");
    let status = unsafe {
        AudioHardwareCreateAggregateDevice(&properties, NonNull::from(&mut guard.aggregate_id))
    };
    if status != 0 {
        return Err(anyhow!("CoreAudio aggregate: OSStatus {status}"));
    }
    let id = cpal::DeviceId::new(cpal::HostId::CoreAudio, aggregate_uid);
    let device = host
        .device_by_id(&id)
        .context("Не удалось открыть CoreAudio tap")?;
    Ok((device, guard))
}

fn to_cfstring(cstr: &'static CStr) -> CFRetained<CFString> {
    unsafe {
        CFString::with_c_string(
            kCFAllocatorDefault,
            cstr.as_ptr(),
            0x08000100, /* UTF8 */
        )
    }
    .unwrap()
}

/// Rust reimplementation of the following:
/// ```c
/// tap_uid = [[tap_description UUID] UUIDString];
/// taps = @[
///     @{
///         @kAudioSubTapUIDKey : (NSString*)tap_uid,
///         @kAudioSubTapDriftCompensationKey : @YES,
///     },
/// ];
///
/// aggregate_device_properties = @{
///     @kAudioAggregateDeviceNameKey : @"MiniMetersAggregateDevice",
///     @kAudioAggregateDeviceUIDKey :
///         @"com.josephlyncheski.MiniMetersAggregateDevice",
///     @kAudioAggregateDeviceTapListKey : taps,
///     @kAudioAggregateDeviceTapAutoStartKey : @YES,
///     @kAudioAggregateDeviceIsPrivateKey : @YES,
/// };
/// ```
fn create_audio_aggregate_device_properties(
    tap_uid: Retained<NSString>,
    agg_uid: &str,
    agg_name: &str,
) -> CFRetained<CFDictionary> {
    let tap_inner = unsafe {
        let dict = CFMutableDictionary::new(
            kCFAllocatorDefault,
            2,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
        .unwrap();

        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioSubTapUIDKey) as *const _ as *const c_void,
            &*tap_uid as *const _ as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioSubTapDriftCompensationKey) as *const _ as *const c_void,
            &*NSNumber::initWithBool(NSNumber::alloc(), true) as *const _ as *const c_void,
        );

        dict
    };
    let _taps_list = [tap_inner];
    let taps = unsafe {
        CFArray::new(
            kCFAllocatorDefault,
            _taps_list.as_ptr() as *mut *const c_void,
            _taps_list.len() as _,
            &kCFTypeArrayCallBacks,
        )
        .unwrap()
    };
    unsafe {
        let dict = CFMutableDictionary::new(
            kCFAllocatorDefault,
            5,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
        .unwrap();

        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceNameKey) as *const _ as *const c_void,
            &*CFString::from_str(agg_name) as *const _ as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceUIDKey) as *const _ as *const c_void,
            &*CFString::from_str(agg_uid) as *const _ as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceTapListKey) as *const _ as *const c_void,
            &*taps as *const _ as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceTapAutoStartKey) as *const _ as *const c_void,
            &*NSNumber::initWithBool(NSNumber::alloc(), true) as *const _ as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceIsPrivateKey) as *const _ as *const c_void,
            &*NSNumber::initWithBool(NSNumber::alloc(), true) as *const _ as *const c_void,
        );

        CFRetained::cast_unchecked::<CFDictionary>(dict)
    }
}
