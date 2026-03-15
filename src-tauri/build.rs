fn main() {
    // Embed Windows application manifest requesting admin privileges
    // Required for LibreHardwareMonitor to access CPU temperature (Ring 0/MSR),
    // storage S.M.A.R.T. data, and motherboard SuperIO sensors.
    #[cfg(target_os = "windows")]
    {
        let mut windows = tauri_build::WindowsAttributes::new();
        windows = windows.app_manifest(include_str!("app.manifest"));
        let attrs = tauri_build::Attributes::new().windows_attributes(windows);
        tauri_build::Builder::new().try_build(attrs).expect("failed to build tauri app");
    }

    #[cfg(not(target_os = "windows"))]
    {
        tauri_build::build()
    }
}
