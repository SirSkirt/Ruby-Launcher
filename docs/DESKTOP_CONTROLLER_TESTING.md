# Desktop Controller Testing

## Recommended test controllers

- Xbox Wireless Controller over USB or Bluetooth
- PlayStation DualSense or DualShock 4
- 8BitDo controller in XInput mode
- Generic controller exposing the standard browser gamepad mapping

## Test checklist

1. Start Ruby Launcher with no controller attached. The connection gate must remain visible.
2. Connect a controller. The gate should disappear and Home should receive focus.
3. Move with both the D-pad and left stick.
4. Use A / Cross to activate buttons.
5. Use LB and RB to cycle through Home, Instances, Versions and Settings.
6. Open a dialog and press B / Circle to close it.
7. Press Start / Menu to open the launcher log.
8. Disconnect the controller. The launcher must lock immediately.
9. Reconnect and verify that navigation resumes.
10. Raise the deadzone if the stick causes unintended focus movement.

## Browser note

Some browsers do not expose a newly connected controller until a button is pressed. This is a browser privacy behavior, not a Ruby Launcher error.

## Android TV note

Android WebView gamepad support will be tested separately. If it proves inconsistent across televisions and streaming boxes, a small native Kotlin adapter can pass controller state into the same JavaScript navigation layer.
