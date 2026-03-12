; TGG NSIS installer hooks
; Clean up AppData on uninstall so users get a fresh start after reinstall

!macro NSIS_HOOK_PREUNINSTALL
  ; Ask user if they want to remove app data
  MessageBox MB_YESNO "Do you want to remove all application data (documents, chats, settings, downloaded models)?$\n$\nClick Yes for a clean uninstall, No to keep your data." IDYES removeData IDNO skipRemove

  removeData:
    ; Remove Roaming app data (database, settings, models)
    RMDir /r "$APPDATA\com.vectorless.rag"
    RMDir /r "$APPDATA\vectorless-rag"

    ; Remove Local app data (WebView cache)
    RMDir /r "$LOCALAPPDATA\com.vectorless.rag"
    RMDir /r "$LOCALAPPDATA\vectorless-rag"

  skipRemove:
!macroend
