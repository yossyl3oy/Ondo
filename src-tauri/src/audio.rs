use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub device_type: String, // "playback" or "recording"
    /// Shared-mode format the device runs at (what every app's audio is
    /// resampled to), i.e. the "Default Format" in Windows sound settings.
    #[serde(default)]
    pub format: Option<AudioFormat>,
    /// How the device is attached: "Bluetooth", "USB", "HDMI/DP", "S/PDIF", "Built-in"
    #[serde(default)]
    pub connection: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate: u32,
    /// Valid bits (e.g. 24 for 24-in-32 containers)
    pub bits_per_sample: u16,
    pub channels: u16,
}

const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// Parses a WAVEFORMATEX / WAVEFORMATEXTENSIBLE blob (as stored in
/// PKEY_AudioEngine_DeviceFormat). Little-endian layout:
///   0 wFormatTag u16, 2 nChannels u16, 4 nSamplesPerSec u32,
///   14 wBitsPerSample u16, 16 cbSize u16, 18 wValidBitsPerSample u16 (EXTENSIBLE)
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_wave_format(blob: &[u8]) -> Option<AudioFormat> {
    let u16_at = |off: usize| blob.get(off..off + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |off: usize| {
        blob.get(off..off + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };

    let tag = u16_at(0)?;
    let channels = u16_at(2)?;
    let sample_rate = u32_at(4)?;
    let container_bits = u16_at(14)?;
    if channels == 0 || sample_rate == 0 || container_bits == 0 {
        return None;
    }

    let valid_bits = if tag == WAVE_FORMAT_EXTENSIBLE && u16_at(16).unwrap_or(0) >= 22 {
        u16_at(18).filter(|&v| v > 0 && v <= container_bits)
    } else {
        None
    };

    Some(AudioFormat {
        sample_rate,
        bits_per_sample: valid_bits.unwrap_or(container_bits),
        channels,
    })
}

// EndpointFormFactor values (mmdeviceapi.h)
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const FORM_FACTOR_SPDIF: i32 = 8;
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const FORM_FACTOR_DIGITAL_DISPLAY: i32 = 9;

/// Classifies the connection from the endpoint form factor and the device
/// id of the adapter its topology connects to (e.g. `{2}.\\?\usb#vid_...`).
/// Form factor wins for display / S/PDIF outputs, which live on onboard or
/// GPU HD Audio controllers that would otherwise look "Built-in".
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn classify_connection(form_factor: Option<i32>, adapter_id: Option<&str>) -> Option<&'static str> {
    match form_factor {
        // DigitalAudioDisplayDevice covers both HDMI and DisplayPort
        Some(FORM_FACTOR_DIGITAL_DISPLAY) => return Some("HDMI/DP"),
        Some(FORM_FACTOR_SPDIF) => return Some("S/PDIF"),
        _ => {}
    }
    let id = adapter_id?.to_ascii_lowercase();
    if id.contains("bthenum") || id.contains("bthhfenum") || id.contains("bthleenum") {
        Some("Bluetooth")
    } else if id.contains("usb") {
        Some("USB")
    } else if id.contains("hdaudio") || id.contains("acpi") {
        // Not "intelaudio": Intel SST also routes Bluetooth headsets through
        // it, so it can't tell built-in from external — show nothing instead
        Some("Built-in")
    } else {
        None
    }
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
                        format: None,
                        connection: None,
                    });
                }

                if has_input {
                    devices.push(AudioDevice {
                        id: dev_id.to_string(),
                        name,
                        is_default: dev_id == default_input_id,
                        device_type: "recording".to_string(),
                        format: None,
                        connection: None,
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
    // IPolicyConfig (below) is a COM interface whose method names must match
    // the Windows vtable verbatim; the macro expands them into multiple items
    // (trait, vtable struct, wrappers), so silence the lint module-wide.
    #![allow(non_snake_case)]

    use super::{classify_connection, parse_wave_format, AudioDevice, AudioFormat};
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

                let store = device.OpenPropertyStore(STGM_READ).ok();
                let name = store
                    .as_ref()
                    .and_then(|s| s.GetValue(&PKEY_Device_FriendlyName).ok())
                    .map(|prop| prop.to_string().trim().to_string())
                    .unwrap_or_else(|| format!("Audio Device {}", i));
                let format = store.as_ref().and_then(|s| read_device_format(s));
                let form_factor = store.as_ref().and_then(|s| read_form_factor(s));
                let adapter_id = adapter_device_id(&device);
                let connection = classify_connection(form_factor, adapter_id.as_deref())
                    .map(str::to_string);

                let is_default = default_id.as_ref().map(|did| *did == id).unwrap_or(false);

                devices.push(AudioDevice {
                    id,
                    name,
                    is_default,
                    device_type: device_type_str.to_string(),
                    format,
                    connection,
                });
            }

            devices
        }
    }

    // VT_BLOB / VT_UI4 (wtypes.h)
    const VT_BLOB: u16 = 65;
    const VT_UI4: u16 = 19;

    unsafe fn read_device_format(
        store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore,
    ) -> Option<AudioFormat> {
        let prop = store.GetValue(&PKEY_AudioEngine_DeviceFormat).ok()?;
        let inner = &prop.Anonymous.Anonymous;
        if inner.vt.0 != VT_BLOB {
            return None;
        }
        let blob = inner.Anonymous.blob;
        if blob.pBlobData.is_null() || blob.cbSize == 0 {
            return None;
        }
        let bytes = std::slice::from_raw_parts(blob.pBlobData, blob.cbSize as usize);
        parse_wave_format(bytes)
    }

    unsafe fn read_form_factor(
        store: &windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore,
    ) -> Option<i32> {
        let prop = store.GetValue(&PKEY_AudioEndpoint_FormFactor).ok()?;
        let inner = &prop.Anonymous.Anonymous;
        (inner.vt.0 == VT_UI4).then(|| inner.Anonymous.ulVal as i32)
    }

    /// Device id of the adapter (USB / Bluetooth / HD Audio controller) the
    /// endpoint's first topology connector is wired to.
    unsafe fn adapter_device_id(device: &IMMDevice) -> Option<String> {
        let topology: IDeviceTopology = device.Activate(CLSCTX_ALL, None).ok()?;
        let connector = topology.GetConnector(0).ok()?;
        let raw = connector.GetDeviceIdConnectedTo().ok()?;
        let id = raw.to_string().ok();
        CoTaskMemFree(Some(raw.0 as *const _));
        id
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

#[cfg(test)]
mod tests {
    use super::*;

    fn wave_format_ex(tag: u16, channels: u16, rate: u32, bits: u16, cb_size: u16) -> Vec<u8> {
        let block_align = channels * bits / 8;
        let mut b = Vec::new();
        b.extend_from_slice(&tag.to_le_bytes());
        b.extend_from_slice(&channels.to_le_bytes());
        b.extend_from_slice(&rate.to_le_bytes());
        b.extend_from_slice(&(rate * block_align as u32).to_le_bytes());
        b.extend_from_slice(&block_align.to_le_bytes());
        b.extend_from_slice(&bits.to_le_bytes());
        b.extend_from_slice(&cb_size.to_le_bytes());
        b
    }

    fn extensible(channels: u16, rate: u32, container: u16, valid: u16) -> Vec<u8> {
        let mut b = wave_format_ex(WAVE_FORMAT_EXTENSIBLE, channels, rate, container, 22);
        b.extend_from_slice(&valid.to_le_bytes()); // Samples.wValidBitsPerSample
        b.extend_from_slice(&3u32.to_le_bytes()); // dwChannelMask
        b.extend_from_slice(&[0u8; 16]); // SubFormat GUID
        b
    }

    #[test]
    fn parses_plain_pcm() {
        let f = parse_wave_format(&wave_format_ex(1, 2, 44_100, 16, 0)).unwrap();
        assert_eq!(f, AudioFormat { sample_rate: 44_100, bits_per_sample: 16, channels: 2 });
    }

    #[test]
    fn extensible_reports_valid_bits_not_container() {
        let f = parse_wave_format(&extensible(2, 48_000, 32, 24)).unwrap();
        assert_eq!(f, AudioFormat { sample_rate: 48_000, bits_per_sample: 24, channels: 2 });
    }

    #[test]
    fn extensible_with_bogus_valid_bits_falls_back_to_container() {
        let f = parse_wave_format(&extensible(8, 96_000, 24, 0)).unwrap();
        assert_eq!(f.bits_per_sample, 24);
        assert_eq!(f.channels, 8);
    }

    #[test]
    fn truncated_or_empty_blob_is_rejected() {
        assert_eq!(parse_wave_format(&[]), None);
        assert_eq!(parse_wave_format(&wave_format_ex(1, 2, 48_000, 16, 0)[..10]), None);
        assert_eq!(parse_wave_format(&wave_format_ex(1, 0, 48_000, 16, 0)), None);
    }

    #[test]
    fn classifies_connections() {
        let bt = r"{2}.\\?\bthenum#{0000110b-0000-1000-8000-00805f9b34fb}_vid&0001";
        let usb = r"{2}.\\?\usb#vid_2a39&pid_3fd3&mi_00#7&1";
        let hda = r"{2}.\\?\hdaudio#func_01&ven_10ec&dev_0897";
        assert_eq!(classify_connection(Some(1), Some(bt)), Some("Bluetooth"));
        assert_eq!(classify_connection(Some(1), Some(usb)), Some("USB"));
        assert_eq!(classify_connection(Some(1), Some(hda)), Some("Built-in"));
        assert_eq!(classify_connection(Some(1), Some("{2}.something-else")), None);
        // Intel SST hosts both onboard codecs and BT headsets: undecidable
        assert_eq!(classify_connection(Some(5), Some(r"{2}.\\?\intelaudio#subsys_1")), None);
        assert_eq!(classify_connection(Some(1), None), None);
    }

    #[test]
    fn form_factor_overrides_adapter_bus() {
        // GPU HDMI audio sits on an HD Audio controller
        let hda = r"{2}.\\?\hdaudio#func_01&ven_10de&dev_009a";
        assert_eq!(classify_connection(Some(FORM_FACTOR_DIGITAL_DISPLAY), Some(hda)), Some("HDMI/DP"));
        assert_eq!(classify_connection(Some(FORM_FACTOR_SPDIF), None), Some("S/PDIF"));
    }
}
