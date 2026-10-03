param([Parameter(Mandatory=$true)][int]$TestProcessId, [string]$SelectItem = '')
$ErrorActionPreference = 'Stop'
if ($env:NATIVE_TEST_PROFILE -ne '1') { throw 'Requires an isolated test instance' }
Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TrayProbe {
 public delegate bool EnumProc(IntPtr h, IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
 [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr h);
 [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr h, int i);
 [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr h, uint i, uint f);
 [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetMenuString(IntPtr h, uint i, StringBuilder s, int n, uint f);
 public static IntPtr Find(int pid, string cls) {
  IntPtr found=IntPtr.Zero;
  EnumWindows((h,p)=>{uint owner; GetWindowThreadProcessId(h,out owner); if(owner!=pid)return true; var s=new StringBuilder(256); GetClassName(h,s,256); if(s.ToString()==cls){found=h;return false;} return true;},IntPtr.Zero);
  return found;
 }
}
"@
$trayHandle = [TrayProbe]::Find($TestProcessId, 'tray_icon_app')
if ($trayHandle -eq [IntPtr]::Zero) { throw 'Test process has no native tray icon window' }
[TrayProbe]::PostMessage($trayHandle,6002,[IntPtr]::Zero,[IntPtr]0x205) | Out-Null
$menuWindow = [IntPtr]::Zero
for ($attempt=0; $attempt -lt 40; $attempt++) {
 $menuWindow = [TrayProbe]::Find($TestProcessId, '#32768')
 if ($menuWindow -ne [IntPtr]::Zero) { break }
 Start-Sleep -Milliseconds 50
}
if ($menuWindow -eq [IntPtr]::Zero) { throw 'Native tray context menu did not open' }
$menuHandle = [TrayProbe]::SendMessage($menuWindow,0x1E1,[IntPtr]::Zero,[IntPtr]::Zero)
$items = @()
for ($index=0; $index -lt [TrayProbe]::GetMenuItemCount($menuHandle); $index++) {
 $label = New-Object System.Text.StringBuilder 256
 [TrayProbe]::GetMenuString($menuHandle,$index,$label,256,0x400) | Out-Null
 $items += [pscustomobject]@{text=$label.ToString();id=[TrayProbe]::GetMenuItemID($menuHandle,$index);enabled=(([TrayProbe]::GetMenuState($menuHandle,$index,0x400) -band 3) -eq 0)}
}
[TrayProbe]::SendMessage($trayHandle,0x1F,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
if ($SelectItem) {
 $selected = $items | Where-Object {$_.text -eq $SelectItem} | Select-Object -First 1
 if (!$selected -or !$selected.enabled) { throw "Menu item unavailable: $SelectItem" }
 [TrayProbe]::PostMessage($trayHandle,0x111,[IntPtr]([long]$selected.id),[IntPtr]::Zero) | Out-Null
}
$items | ConvertTo-Json -Compress
