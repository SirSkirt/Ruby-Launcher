# Microsoft Sign-in — b2026.07

Ruby Launcher's approved Microsoft Application ID is built into the application. Users do not need to create an Azure/Entra registration or paste a client ID into Settings.

## Sign in

1. Select **Connect account**.
2. Open the Microsoft device-login page shown by Ruby Launcher.
3. Enter the displayed code and sign in with the account that owns Minecraft: Java Edition.
4. After ownership and profile verification, Ruby Launcher asks: **Do you want to stay signed in?**
5. Choose **Yes** to save the refresh token securely, or **No** for a session-only login.

Ruby Launcher never receives the Microsoft password. No client secret is included in the application.

## Remember Me

- Windows builds use Windows Credential Manager.
- Android builds use SharedPreferences encrypted through Android Keystore.
- Choosing No does not write the refresh token to disk.
- Signing out deletes any saved refresh token.
- Microsoft may still require a new login after password/security changes, revocation or refresh-token expiry.
