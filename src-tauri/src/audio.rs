use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub device_type: String, // "playback" or "recording"
}

// ============================================================
// macOS implementation
// ============================================================
#[cfg(target_os = "macos")]
mod platform {
    use super::AudioDevice;
    use coreaudio_sys::*;
    use std::mem;
    use std::ptr;

    unsafe fn get_string_property(
        device_id: AudioDeviceID,
        selector: AudioObjectPropertySelector,
    ) -> Option<String> {
        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };

        let mut name_ref: CFStringRef = ptr::null();
        let mut size = mem::size_of::<CFStringRef>() as u32;

        let status = AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut name_ref as *mut _ as *mut _,
        );

        if status != 0 || name_ref.is_null() {
            return None;
        }

        // Convert CFStringRef to Rust String
        let length = CFStringGetLength(name_ref);
        let max_size = CFStringGetMaximumSizeForEncoding(length, kCFStringEncodingUTF8) + 1;
        let mut buffer = vec![0u8; max_size as usize];
        let success = CFStringGetCString(
            name_ref,
            buffer.as_mut_ptr() as *mut _,
            max_size,
            kCFStringEncodingUTF8,
        );

        CFRelease(name_ref as *const _);

        if success != 0 {
            let c_str = std::ffi::CStr::from_ptr(buffer.as_ptr() as *const _);
            c_str.to_str().ok().map(|s| s.to_string())
        } else {
            None
        }
    }

    unsafe fn device_has_streams(
        device_id: AudioDeviceID,
        scope: AudioObjectPropertyScope,
    ) -> bool {
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyStreams,
            mScope: scope,
            mElement: kAudioObjectPropertyElementMain,
        };

        let mut size: u32 = 0;
        let status = AudioObjectGetPropertyDataSize(device_id, &address, 0, ptr::null(), &mut size);

        status == 0 && size > 0
    }

    unsafe fn get_default_device(selector: AudioObjectPropertySelector) -> AudioDeviceID {
        let address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };

        let mut device_id: AudioDeviceID = kAudioObjectUnknown;
        let mut size = mem::size_of::<AudioDeviceID>() as u32;

        let status = AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &address,
            0,
            ptr::null(),
            &mut size,
            &mut device_id as *mut _ as *mut _,
        );

        if status == 0 {
            device_id
        } else {
            kAudioObjectUnknown
        }
    }

    pub fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
        unsafe {
            let address = AudioObjectPropertyAddress {
                mSelector: kAudioHardwarePropertyDevices,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain,
            };

            let mut size: u32 = 0;
            let status = AudioObjectGetPropertyDataSize(
                kAudioObjectSystemObject,
                &address,
                0,
                ptr::null(),
                &mut size,
            );

            if status != 0 {
                return Err(format!("Failed to get device list size: {}", status));
            }

            let device_count = size as usize / mem::size_of::<AudioDeviceID>();
            let mut device_ids = vec![0u32; device_count];

            let status = AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &address,
                0,
                ptr::null(),
                &mut size,
                device_ids.as_mut_ptr() as *mut _,
            );

            if status != 0 {
                return Err(format!("Failed to get device list: {}", status));
            }

            let default_output_id = get_default_device(kAudioHardwarePropertyDefaultOutputDevice);
            let default_input_id = get_default_device(kAudioHardwarePropertyDefaultInputDevice);
            let mut devices = Vec::new();

            for &dev_id in &device_ids {
                let name = get_string_property(dev_id, kAudioObjectPropertyName)
                    .unwrap_or_else(|| format!("Unknown Device {}", dev_id));

                let has_output = device_has_streams(dev_id, kAudioObjectPropertyScopeOutput);
                let has_input = device_has_streams(dev_id, kAudioObjectPropertyScopeInput);

                if has_output {
                    devices.push(AudioDevice {
                        id: dev_id.to_string(),
                        name: name.clone(),
                        is_default: dev_id == default_output_id,
                        device_type: "playback".to_string(),
                    });
                }

                if has_input {
                    devices.push(AudioDevice {
                        id: dev_id.to_string(),
                        name,
                        is_default: dev_id == default_input_id,
                        device_type: "recording".to_string(),
                    });
                }
            }

            Ok(devices)
        }
    }

    pub fn set_default_audio_device(device_id: &str, device_type: &str) -> Result<(), String> {
        let dev_id: AudioDeviceID = device_id
            .parse()
            .map_err(|_| format!("Invalid device ID: {}", device_id))?;

        let selector = if device_type == "recording" {
            kAudioHardwarePropertyDefaultInputDevice
        } else {
            kAudioHardwarePropertyDefaultOutputDevice
        };

        unsafe {
            let address = AudioObjectPropertyAddress {
                mSelector: selector,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain,
            };

            let size = mem::size_of::<AudioDeviceID>() as u32;
            let status = AudioObjectSetPropertyData(
                kAudioObjectSystemObject,
                &address,
                0,
                ptr::null(),
                size,
                &dev_id as *const _ as *const _,
            );

            if status != 0 {
                Err(format!(
                    "Failed to set default {} device: {}",
                    device_type, status
                ))
            } else {
                Ok(())
            }
        }
    }
}

// ============================================================
// Windows implementation
// ============================================================
#[cfg(target_os = "windows")]
mod platform {
    use super::AudioDevice;
    use windows::core::{GUID, HRESULT, PCWSTR};
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Com::*;

    // IPolicyConfig is an undocumented COM interface used to set default audio device
    #[windows::core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    unsafe trait IPolicyConfig: windows::core::IUnknown {
        unsafe fn GetMixFormat(&self, device_id: PCWSTR, format: *mut *mut WAVEFORMATEX)
            -> HRESULT;
        unsafe fn GetDeviceFormat(
            &self,
            device_id: PCWSTR,
            default: i32,
            format: *mut *mut WAVEFORMATEX,
        ) -> HRESULT;
        unsafe fn ResetDeviceFormat(&self, device_id: PCWSTR) -> HRESULT;
        unsafe fn SetDeviceFormat(
            &self,
            device_id: PCWSTR,
            endpoint_format: *const WAVEFORMATEX,
            mix_format: *const WAVEFORMATEX,
        ) -> HRESULT;
        unsafe fn GetProcessingPeriod(
            &self,
            device_id: PCWSTR,
            default: i32,
            default_period: *mut i64,
            min_period: *mut i64,
        ) -> HRESULT;
        unsafe fn SetProcessingPeriod(&self, device_id: PCWSTR, period: *const i64) -> HRESULT;
        unsafe fn GetShareMode(&self, device_id: PCWSTR, mode: *mut DeviceShareMode) -> HRESULT;
        unsafe fn SetShareMode(&self, device_id: PCWSTR, mode: *const DeviceShareMode) -> HRESULT;
        unsafe fn GetPropertyValue(
            &self,
            device_id: PCWSTR,
            store_flag: i32,
            key: *const PROPERTYKEY,
            value: *mut PROPVARIANT,
        ) -> HRESULT;
        unsafe fn SetPropertyValue(
            &self,
            device_id: PCWSTR,
            store_flag: i32,
            key: *const PROPERTYKEY,
            value: *const PROPVARIANT,
        ) -> HRESULT;
        unsafe fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
        unsafe fn SetEndpointVisibility(&self, device_id: PCWSTR, visible: i32) -> HRESULT;
    }

    // DeviceShareMode placeholder
    #[repr(C)]
    struct DeviceShareMode {
        _data: [u8; 4],
    }

    const CLSID_POLICY_CONFIG_CLIENT: GUID =
        GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

    fn enumerate_devices(
        enumerator: &IMMDeviceEnumerator,
        data_flow: EDataFlow,
        device_type_str: &str,
        default_id: &Option<String>,
    ) -> Vec<AudioDevice> {
        unsafe {
            let collection = match enumerator.EnumAudioEndpoints(data_flow, DEVICE_STATE_ACTIVE) {
                Ok(c) => c,
                Err(_) => return Vec::new(),
            };

            let count = match collection.GetCount() {
                Ok(c) => c,
                Err(_) => return Vec::new(),
            };

            let mut devices = Vec::new();

            for i in 0..count {
                let device = match collection.Item(i) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                let id = match device.GetId() {
                    Ok(id) => {
                        let s = id.to_string().unwrap_or_default();
                        s
                    }
                    Err(_) => continue,
                };

                let name = match device.OpenPropertyStore(STGM_READ) {
                    Ok(store) => match store.GetValue(&PKEY_Device_FriendlyName) {
                        Ok(prop) => prop.to_string().trim().to_string(),
                        Err(_) => format!("Audio Device {}", i),
                    },
                    Err(_) => format!("Audio Device {}", i),
                };

                let is_default = default_id.as_ref().map(|did| *did == id).unwrap_or(false);

                devices.push(AudioDevice {
                    id,
                    name,
                    is_default,
                    device_type: device_type_str.to_string(),
                });
            }

            devices
        }
    }

    pub fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
        unsafe {
            // Initialize COM
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create device enumerator: {}", e))?;

            // Get default output device
            let default_output_id = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .ok()
                .and_then(|d| d.GetId().ok().map(|id| id.to_string().unwrap_or_default()));

            // Get default input device
            let default_input_id = enumerator
                .GetDefaultAudioEndpoint(eCapture, eConsole)
                .ok()
                .and_then(|d| d.GetId().ok().map(|id| id.to_string().unwrap_or_default()));

            let mut devices =
                enumerate_devices(&enumerator, eRender, "playback", &default_output_id);
            devices.extend(enumerate_devices(
                &enumerator,
                eCapture,
                "recording",
                &default_input_id,
            ));

            Ok(devices)
        }
    }

    pub fn set_default_audio_device(device_id: &str, _device_type: &str) -> Result<(), String> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let policy_config: IPolicyConfig =
                CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
                    .map_err(|e| format!("Failed to create PolicyConfig: {}", e))?;

            let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();

            let hr = policy_config.SetDefaultEndpoint(PCWSTR(wide_id.as_ptr()), eConsole);

            if hr.is_err() {
                return Err(format!("Failed to set default device: {:?}", hr));
            }

            // Also set for eMultimedia and eCommunications roles
            let _ = policy_config.SetDefaultEndpoint(PCWSTR(wide_id.as_ptr()), eMultimedia);
            let _ = policy_config.SetDefaultEndpoint(PCWSTR(wide_id.as_ptr()), eCommunications);

            Ok(())
        }
    }
}

// ============================================================
// Linux stub (unsupported)
// ============================================================
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::AudioDevice;

    pub fn get_audio_devices() -> Result<Vec<AudioDevice>, String> {
        Err("Audio device switching is not supported on this platform".to_string())
    }

    pub fn set_default_audio_device(_device_id: &str, _device_type: &str) -> Result<(), String> {
        Err("Audio device switching is not supported on this platform".to_string())
    }
}

pub use platform::{get_audio_devices, set_default_audio_device};
