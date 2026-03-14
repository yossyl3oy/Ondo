const fs = require("fs");

const version = JSON.parse(fs.readFileSync("package.json", "utf8")).version;

// Sync Cargo.toml
const cargoPath = "src-tauri/Cargo.toml";
let cargo = fs.readFileSync(cargoPath, "utf8");
cargo = cargo.replace(/^version = ".*"/m, `version = "${version}"`);
fs.writeFileSync(cargoPath, cargo);

// Sync tauri.conf.json
const tauriPath = "src-tauri/tauri.conf.json";
const tauri = JSON.parse(fs.readFileSync(tauriPath, "utf8"));
tauri.version = version;
fs.writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + "\n");

console.log(`Synced version ${version} → Cargo.toml, tauri.conf.json`);
