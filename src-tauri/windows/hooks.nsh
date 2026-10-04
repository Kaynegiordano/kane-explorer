; Kane Explorer : intégration à Windows pendant l'installation / la désinstallation.
; Tout est écrit pour l'utilisateur courant (HKCU) : aucun droit administrateur nécessaire.

!define KANE_EXE "$INSTDIR\kane-explorer.exe"
!define KANE_WIN_E "Software\Classes\CLSID\{52205fd8-5dfb-447d-801a-d0b52f2e83e1}"

!macro KANE_REGISTER_CLASS CLASS
  WriteRegStr HKCU "Software\Classes\${CLASS}\shell" "" "KaneExplorer"
  WriteRegStr HKCU "Software\Classes\${CLASS}\shell\KaneExplorer" "" "Ouvrir dans Kane Explorer"
  WriteRegStr HKCU "Software\Classes\${CLASS}\shell\KaneExplorer" "Icon" "${KANE_EXE}"
  WriteRegStr HKCU "Software\Classes\${CLASS}\shell\KaneExplorer\command" "" '"${KANE_EXE}" "%1"'
!macroend

!macro KANE_UNREGISTER_CLASS CLASS
  ReadRegStr $0 HKCU "Software\Classes\${CLASS}\shell" ""
  StrCmp $0 "KaneExplorer" 0 +2
    DeleteRegValue HKCU "Software\Classes\${CLASS}\shell" ""
  DeleteRegKey HKCU "Software\Classes\${CLASS}\shell\KaneExplorer"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Remplacer l'Explorateur Windows ? (réversible à tout moment dans Kane : Options des dossiers)
  MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON1 "Faire de Kane Explorer votre explorateur de fichiers par défaut ?$\r$\n$\r$\n• Windows + E ouvrira Kane Explorer$\r$\n• Les dossiers et les disques s'ouvriront dans Kane Explorer$\r$\n$\r$\nVous pourrez revenir à l'Explorateur Windows à tout moment dans Kane Explorer (Options des dossiers), et il est rétabli automatiquement si vous désinstallez Kane Explorer." /SD IDNO IDNO kane_skip_default
    !insertmacro KANE_REGISTER_CLASS "Directory"
    !insertmacro KANE_REGISTER_CLASS "Drive"
    WriteRegStr HKCU "${KANE_WIN_E}\shell\opennewwindow\command" "" '"${KANE_EXE}"'
    WriteRegStr HKCU "${KANE_WIN_E}\shell\opennewwindow\command" "DelegateExecute" ""
  kane_skip_default:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Rétablit l'Explorateur Windows
  !insertmacro KANE_UNREGISTER_CLASS "Directory"
  !insertmacro KANE_UNREGISTER_CLASS "Drive"
  DeleteRegKey HKCU "${KANE_WIN_E}"
!macroend
