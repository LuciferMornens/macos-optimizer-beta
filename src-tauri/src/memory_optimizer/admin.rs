// src/memory_optimizer/admin.rs

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

pub(crate) struct AdminScriptOutcome {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub cancelled: bool,
}

pub(crate) fn run_deep_clean() -> AdminScriptOutcome {
    let script_path = "/tmp/macos_optimizer_deep_clean.sh";
    let shell_script = r#"#!/bin/bash
set -euo pipefail

# Helper to run a step and echo a marker
run() {
  local label="$1"; shift
  if "$@"; then
    echo "OK:${label}"
  else
    echo "ERR:${label}"
  fi
}

# Admin-required tasks (with markers)
run PURGE purge
run DNS dscacheutil -flushcache
run MDNS killall -HUP mDNSResponder
run CLEAR_SYS_CACHE bash -lc 'rm -rf /Library/Caches/* && rm -rf /private/var/folders/*/C/* && rm -rf /private/var/folders/*/*/com.apple.LaunchServices*'
run CLEAR_SWAP bash -lc 'rm -f /private/var/vm/swapfile*'
run LSREGISTER "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister" -kill -r -domain local -domain system -domain user
run ATSUTIL atsutil databases -remove
run KEXT_TOUCH touch /System/Library/Extensions
run KEXTCACHE kextcache -update-volume /
run PERIODIC periodic daily weekly monthly

# Restart common UI services (markers too)
run RESTART_Dock killall -KILL Dock
run RESTART_Finder killall -KILL Finder
run RESTART_SysUIS killall -KILL SystemUIServer
run RESTART_cfprefsd killall cfprefsd
"#;

    // Write script to disk and make it executable
    let _ = fs::write(script_path, shell_script);
    if let Ok(meta) = fs::metadata(script_path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(script_path, perms);
    }

    // Single admin prompt for the whole run
    let applescript = format!(r#"with timeout of 1200 seconds
  do shell script "{}" with administrator privileges
end timeout"#, script_path);

    let result = Command::new("osascript").arg("-e").arg(applescript).output();

    // Always attempt cleanup of the temp script
    let _ = fs::remove_file(script_path);

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let cancelled = stderr.contains("canceled") || stderr.contains("cancelled") || stderr.contains("-128");
            AdminScriptOutcome {
                success: output.status.success(),
                stdout,
                stderr,
                cancelled,
            }
        }
        Err(e) => {
            AdminScriptOutcome {
                success: false,
                stdout: String::new(),
                stderr: format!("Failed to run admin script: {}", e),
                cancelled: false,
            }
        }
    }
}