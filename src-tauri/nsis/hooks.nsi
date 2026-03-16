!macro NSIS_HOOK_PREINSTALL
  ; Kill running Ondo processes before installing to avoid "file in use" errors
  nsExec::ExecToLog 'taskkill /f /im Ondo.exe'
  nsExec::ExecToLog 'taskkill /f /im ondo-hwmon.exe'
  ; Brief wait to ensure file handles are released
  Sleep 1000
!macroend
