use crate::hardware;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const PORT: u16 = 19210;

pub async fn start_debug_server() {
    let addr = format!("0.0.0.0:{}", PORT);
    let listener = match TcpListener::bind(&addr).await {
        Ok(l) => {
            eprintln!("[DebugServer] Listening on http://{}", addr);
            l
        }
        Err(e) => {
            eprintln!("[DebugServer] Failed to bind {}: {}", addr, e);
            return;
        }
    };

    loop {
        let (mut stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                eprintln!("[DebugServer] Accept error: {}", e);
                continue;
            }
        };

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            let n = match stream.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => return,
            };

            let request = String::from_utf8_lossy(&buf[..n]);
            let path = parse_request_path(&request);

            eprintln!("[DebugServer] {} -> {}", peer, path);

            let response = match path.as_str() {
                "/api/hardware" => handle_hardware().await,
                "/api/sensors" => handle_sensors().await,
                "/" => handle_dashboard(),
                _ => http_response(404, "text/plain", "Not Found"),
            };

            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}

fn parse_request_path(request: &str) -> String {
    // Parse "GET /path HTTP/1.1"
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string()
}

fn http_response(status: u16, content_type: &str, body: &str) -> String {
    let status_text = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        status,
        status_text,
        content_type,
        body.len(),
        body
    )
}

async fn handle_hardware() -> String {
    match hardware::get_hardware_info().await {
        Ok(data) => {
            let json = serde_json::to_string_pretty(&data).unwrap_or_else(|e| {
                format!("{{\"error\": \"Serialization failed: {}\"}}", e)
            });
            http_response(200, "application/json", &json)
        }
        Err(e) => {
            let json = format!("{{\"error\": \"{}\"}}", e);
            http_response(500, "application/json", &json)
        }
    }
}

async fn handle_sensors() -> String {
    let output = get_raw_sensor_data().await;
    http_response(200, "text/plain", &output)
}

#[cfg(target_os = "windows")]
async fn get_raw_sensor_data() -> String {
    use std::process::Stdio;
    use tokio::process::Command;

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return format!("Failed to get exe path: {}", e),
    };
    let exe_dir = match exe_path.parent() {
        Some(d) => d,
        None => return "Failed to get exe directory".to_string(),
    };
    let lhm_path = exe_dir.join("ondo-hwmon.exe");

    if !lhm_path.exists() {
        return format!(
            "ondo-hwmon.exe not found at {:?}\n\n\
             This endpoint requires the LHM CLI binary.\n\
             In development, build it first:\n\
             cd src-lhm && dotnet publish -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -o ../src-tauri/",
            lhm_path
        );
    }

    match Command::new(&lhm_path)
        .arg("--debug")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() {
                stdout.to_string()
            } else {
                format!(
                    "ondo-hwmon.exe --debug failed (exit: {:?})\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
                    output.status.code(),
                    stdout,
                    stderr
                )
            }
        }
        Err(e) => format!("Failed to run ondo-hwmon.exe: {}", e),
    }
}

#[cfg(target_os = "macos")]
async fn get_raw_sensor_data() -> String {
    use sysinfo::Components;

    let components = Components::new_with_refreshed_list();
    let mut output = String::from("=== macOS Temperature Sensors (via sysinfo) ===\n\n");

    for comp in components.iter() {
        output.push_str(&format!(
            "  {:<30} = {:>6.1}°C  (max: {:.1}°C)\n",
            comp.label(),
            comp.temperature(),
            comp.max(),
        ));
    }

    if components.iter().count() == 0 {
        output.push_str("  (no temperature sensors found)\n");
    }

    output
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
async fn get_raw_sensor_data() -> String {
    "Sensor debug not implemented for this platform".to_string()
}

fn handle_dashboard() -> String {
    let html = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Ondo Debug</title>
<style>
  body { background: #1a1a2e; color: #e0e0e0; font-family: 'Courier New', monospace; margin: 20px; }
  h1 { color: #0ff; font-size: 18px; }
  h2 { color: #0af; font-size: 14px; margin-top: 20px; }
  .section { background: #16213e; border: 1px solid #0a3d62; border-radius: 6px; padding: 12px; margin: 8px 0; }
  .label { color: #888; font-size: 11px; }
  .value { color: #0ff; font-size: 16px; font-weight: bold; }
  .temp { color: #0f0; }
  .temp.warn { color: #ff0; }
  .temp.danger { color: #f00; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 8px; }
  .error { color: #f44; }
  pre { background: #0d1b2a; padding: 10px; border-radius: 4px; overflow-x: auto; font-size: 12px; max-height: 400px; overflow-y: auto; }
  .tabs { display: flex; gap: 4px; margin-bottom: 12px; }
  .tab { padding: 6px 14px; background: #16213e; border: 1px solid #0a3d62; border-radius: 4px 4px 0 0; cursor: pointer; color: #888; }
  .tab.active { background: #0a3d62; color: #0ff; }
  .status { font-size: 11px; color: #666; }
  a { color: #0af; }
</style>
</head>
<body>
<h1>&#x25C8; ONDO DEBUG DASHBOARD</h1>
<div class="status">Auto-refresh: 2s | <a href="/api/hardware">JSON</a> | <a href="/api/sensors">Raw Sensors</a></div>

<div class="tabs">
  <div class="tab active" onclick="showTab('hardware')">Hardware Data</div>
  <div class="tab" onclick="showTab('sensors')">Raw Sensors</div>
</div>

<div id="hardware-tab"></div>
<div id="sensors-tab" style="display:none"><pre id="sensors-pre">Loading...</pre></div>

<script>
function tempClass(t, max) {
  const r = t / max;
  return r >= 0.9 ? 'temp danger' : r >= 0.75 ? 'temp warn' : 'temp';
}

function renderHardware(d) {
  let h = '';
  if (d.error) { return '<div class="error">Error: ' + d.error + '</div>'; }

  if (d.cpu) {
    h += '<h2>CPU: ' + d.cpu.name + '</h2><div class="section"><div class="grid">';
    h += '<div><div class="label">Temperature</div><div class="value ' + tempClass(d.cpu.temperature, d.cpu.maxTemperature) + '">' + d.cpu.temperature.toFixed(1) + '°C</div></div>';
    h += '<div><div class="label">Load</div><div class="value">' + d.cpu.load.toFixed(1) + '%</div></div>';
    h += '<div><div class="label">Frequency</div><div class="value">' + d.cpu.frequency.toFixed(2) + ' GHz</div></div>';
    h += '<div><div class="label">Max Temp (TjMax)</div><div class="value">' + d.cpu.maxTemperature + '°C</div></div>';
    h += '</div>';
    if (d.cpu.cores && d.cpu.cores.length) {
      h += '<h2 style="margin-top:10px">Cores</h2><div class="grid">';
      d.cpu.cores.forEach(c => {
        h += '<div><span class="label">Core ' + c.index + '</span> <span class="' + tempClass(c.temperature, d.cpu.maxTemperature) + '">' + c.temperature.toFixed(1) + '°C</span> <span class="value">' + c.load.toFixed(0) + '%</span></div>';
      });
      h += '</div>';
    }
    h += '</div>';
  }

  if (d.gpu) {
    h += '<h2>GPU: ' + d.gpu.name + '</h2><div class="section"><div class="grid">';
    h += '<div><div class="label">Temperature</div><div class="value ' + tempClass(d.gpu.temperature, d.gpu.maxTemperature) + '">' + d.gpu.temperature.toFixed(1) + '°C</div></div>';
    h += '<div><div class="label">Load</div><div class="value">' + d.gpu.load.toFixed(1) + '%</div></div>';
    h += '<div><div class="label">Frequency</div><div class="value">' + d.gpu.frequency.toFixed(2) + ' GHz</div></div>';
    h += '<div><div class="label">VRAM</div><div class="value">' + d.gpu.memoryUsed.toFixed(1) + ' / ' + d.gpu.memoryTotal.toFixed(1) + ' GB</div></div>';
    h += '</div></div>';
  }

  if (d.storage) {
    d.storage.forEach(s => {
      h += '<h2>Storage: ' + s.name + '</h2><div class="section"><div class="grid">';
      h += '<div><div class="label">Temperature</div><div class="value ' + (s.temperature > 0 ? tempClass(s.temperature, 70) : '') + '">' + (s.temperature > 0 ? s.temperature.toFixed(1) + '°C' : 'N/A') + '</div></div>';
      h += '<div><div class="label">Used</div><div class="value">' + s.usedSpace.toFixed(1) + '%</div></div>';
      h += '<div><div class="label">Total</div><div class="value">' + s.totalSpace.toFixed(0) + ' GB</div></div>';
      h += '</div></div>';
    });
  }

  if (d.motherboard) {
    h += '<h2>Motherboard: ' + d.motherboard.name + '</h2><div class="section"><div class="grid">';
    h += '<div><div class="label">Temperature</div><div class="value ' + (d.motherboard.temperature > 0 ? tempClass(d.motherboard.temperature, 80) : '') + '">' + (d.motherboard.temperature > 0 ? d.motherboard.temperature.toFixed(1) + '°C' : 'N/A') + '</div></div>';
    if (d.motherboard.fans) {
      d.motherboard.fans.forEach(f => {
        h += '<div><div class="label">' + f.name + '</div><div class="value">' + f.speed + ' RPM</div></div>';
      });
    }
    h += '</div></div>';
  }

  if (d.cpuError) h += '<div class="error">CPU Error: ' + d.cpuError + '</div>';
  if (d.gpuError) h += '<div class="error">GPU Error: ' + d.gpuError + '</div>';

  return h;
}

function showTab(name) {
  document.getElementById('hardware-tab').style.display = name === 'hardware' ? '' : 'none';
  document.getElementById('sensors-tab').style.display = name === 'sensors' ? '' : 'none';
  document.querySelectorAll('.tab').forEach((t, i) => t.classList.toggle('active', (i === 0 && name === 'hardware') || (i === 1 && name === 'sensors')));
  if (name === 'sensors') fetchSensors();
}

async function fetchHardware() {
  try {
    const r = await fetch('/api/hardware');
    const d = await r.json();
    document.getElementById('hardware-tab').innerHTML = renderHardware(d);
  } catch (e) {
    document.getElementById('hardware-tab').innerHTML = '<div class="error">Fetch error: ' + e + '</div>';
  }
}

async function fetchSensors() {
  try {
    const r = await fetch('/api/sensors');
    document.getElementById('sensors-pre').textContent = await r.text();
  } catch (e) {
    document.getElementById('sensors-pre').textContent = 'Fetch error: ' + e;
  }
}

fetchHardware();
setInterval(fetchHardware, 2000);
</script>
</body>
</html>"#;
    http_response(200, "text/html", html)
}
