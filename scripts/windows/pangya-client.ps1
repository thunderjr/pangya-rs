# PangYa U.S. 852 client automation for pangya-rs verification.
#
# Deployed to the Windows VM as C:\tools\pangya-client.ps1 and dot-sourced from there. Kept under
# version control so the harness travels with the server it drives; copy it back to the VM after
# editing, and copy the VM's copy here after editing it there.
#
# Five constraints shape everything below, each of which cost a debugging session:
#
# 1. The client reads its mouse through DirectInput. It ignores SetCursorPos entirely, so
#    ordinary synthetic clicks never reach its widgets. The only thing that works is RELATIVE
#    SendInput deltas: pin the cursor into a corner with a large negative delta, then move by the
#    target offset. After pinning, engine coordinates equal client-area pixels.
#
# 2. Because the OS cursor ends up at the same coordinates as the engine cursor, the real click
#    lands wherever that point is on screen. With the window at its default placement that point
#    is OUTSIDE the game, so the click activates another window and the client dismisses its
#    modal login dialog. Moving the window to the screen origin keeps every click inside it.
#    This is why login "randomly" stopped working before.
#
# 3. Keyboard input must be SendInput with scan codes. SendKeys targets the foreground window and
#    will silently go to the wrong place (and steal focus, closing the dialog).
#
# 4. The engine drops synthetic clicks often enough that fire-and-forget is the single biggest
#    source of stuck runs: a missed click looks exactly like a server that never replied. Steps
#    therefore assert the screen they are meant to produce and retry.
#
# 5. With two instances running, input focus decides which client acts on the synthetic input,
#    and the focused client cannot be dispossessed by any polite means: it holds its DirectInput
#    devices EXCLUSIVELY, which suppresses Alt+Tab, Alt+Esc and Win shortcuts (synthesised or
#    real), eats clicks aimed at other windows' title bars and at the taskbar, and the foreground
#    lock refuses SetForegroundWindow from this process on top of that. All of those were tried
#    and verified useless. The one thing that works is making the holder LET GO: minimising a
#    DirectInput game forces it to unacquire, and minimising the foreground window releases the
#    foreground lock — see Set-PangyaTarget. GetForegroundWindow reports stale values from this
#    context, so a switch is verified by pixels, never by asking who is foreground.
#
# Every coordinate here is a client-area pixel of a window sitting at the screen ORIGIN, and only
# one window can be there at a time. See docs/RUNNING_THE_CLIENT.md for the surrounding procedure.

Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;using System.Text;using System.Threading;
public class PangyaClient {
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk; public ushort wScan; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public MOUSEINPUT mi; [FieldOffset(8)] public KEYBDINPUT ki; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

  [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] p, int size);
  [DllImport("user32.dll", CharSet=CharSet.Auto)] static extern IntPtr FindWindow(string cls, string name);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] static extern short VkKeyScan(char c);
  [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint type);

  const uint MOVE=0x0001, LDOWN=0x0002, LUP=0x0004;
  const uint KEYUP=0x0002, SCANCODE=0x0008;
  const uint SWP_NOSIZE=0x0001, SWP_NOZORDER=0x0004;

  public static IntPtr Window() { return FindWindow("PangYa", null); }

  // Park the window at the screen origin so every synthetic click lands inside it.
  public static bool Anchor() {
    IntPtr h = Window();
    if (h == IntPtr.Zero) return false;
    SetWindowPos(h, IntPtr.Zero, 0, 0, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
    SetForegroundWindow(h);
    Thread.Sleep(300);
    return true;
  }

  // Screen position of the client area's top-left corner.
  public static POINT Origin() {
    POINT p; p.X = 0; p.Y = 0;
    IntPtr h = Window();
    if (h != IntPtr.Zero) ClientToScreen(h, ref p);
    return p;
  }

  public static POINT ClientSize() {
    RECT r; POINT s; s.X = 0; s.Y = 0;
    IntPtr h = Window();
    if (h != IntPtr.Zero && GetClientRect(h, out r)) { s.X = r.Right; s.Y = r.Bottom; }
    return s;
  }

  static void Mouse(uint flags, int dx, int dy) {
    INPUT[] i = new INPUT[1];
    i[0].type = 0; i[0].mi.dx = dx; i[0].mi.dy = dy; i[0].mi.dwFlags = flags;
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }

  // Move to a client-area pixel. Pin into the corner first so the engine's accumulated
  // position is known, then apply the offset.
  public static void Goto(int cx, int cy) {
    Mouse(MOVE, -6000, -6000); Thread.Sleep(100);
    Mouse(MOVE, cx, cy); Thread.Sleep(250);
  }

  public static void Tap() { Mouse(LDOWN,0,0); Thread.Sleep(120); Mouse(LUP,0,0); Thread.Sleep(300); }
  public static void Click(int cx, int cy) { Goto(cx, cy); Tap(); }
  // A double click must land inside the OS double-click time (500 ms by default). Tap()'s
  // trailing settle delay pushed the second press past it, so the list row only ever selected.
  public static void DoubleClick(int cx, int cy) {
    Goto(cx, cy);
    Mouse(LDOWN,0,0); Thread.Sleep(40); Mouse(LUP,0,0); Thread.Sleep(60);
    Mouse(LDOWN,0,0); Thread.Sleep(40); Mouse(LUP,0,0); Thread.Sleep(400);
  }

  static void Key(ushort scan, bool shift, bool up) {
    INPUT[] i = new INPUT[1];
    i[0].type = 1; i[0].ki.wVk = 0; i[0].ki.wScan = scan;
    i[0].ki.dwFlags = SCANCODE | (up ? KEYUP : 0);
    SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
  }

  // Scan-code keystrokes reach the focused in-engine widget; SendKeys does not.
  public static void Text(string value) {
    foreach (char c in value) {
      short vks = VkKeyScan(c);
      if (vks == -1) continue;
      ushort vk = (ushort)(vks & 0xff);
      bool shift = (vks & 0x100) != 0;
      ushort scan = (ushort)MapVirtualKey(vk, 0);
      if (shift) Key(0x2a, false, false);
      Key(scan, shift, false); Thread.Sleep(25);
      Key(scan, shift, true);  Thread.Sleep(25);
      if (shift) Key(0x2a, false, true);
    }
    Thread.Sleep(200);
  }
}
"@ -ErrorAction SilentlyContinue

# ---- Multi-instance window plumbing --------------------------------------------------------
#
# Anchor() and Origin() above resolve the window with FindWindow, which picks an arbitrary
# instance once two clients are running: clicks then go to the wrong client and screen waits
# sample the wrong window and pass falsely. Everything below resolves a window per process.
#
# The rule for two instances: every instance that is not being driven sits MINIMISED, the one
# being driven sits restored at the origin, and only then does anything click. Minimised is not
# just parking - it is what forces a client to release its exclusive DirectInput grip (see
# constraint 5 in the header). Switching instances mid-flow is an explicit Set-PangyaTarget
# call, not something the click helpers can infer, and driving the wrong window is silent
# rather than an error.
Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;using System.Text;using System.Collections.Generic;
public class PangyaWindows {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Auto)] static extern int GetClassName(IntPtr h, StringBuilder s, int m);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll", EntryPoint="IsIconic")] public static extern bool IsMinimized(IntPtr h);
  [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
  const uint SWP_NOSIZE = 0x0001, SWP_NOZORDER = 0x0004;
  const int SW_MINIMIZE = 6, SW_RESTORE = 9;

  public static List<IntPtr> Find(uint wantPid) {
    var found = new List<IntPtr>();
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (wantPid != 0 && pid != wantPid) return true;
      var sb = new StringBuilder(64);
      GetClassName(h, sb, 64);
      if (sb.ToString() == "PangYa" && IsWindowVisible(h)) found.Add(h);
      return true;
    }, IntPtr.Zero);
    return found;
  }

  public static void Move(IntPtr h, int x, int y) {
    SetWindowPos(h, IntPtr.Zero, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER);
  }

  public static void Minimize(IntPtr h) { ShowWindow(h, SW_MINIMIZE); }
  public static void Restore(IntPtr h)  { ShowWindow(h, SW_RESTORE); }

  // Screen position of a SPECIFIC window's client area, unlike PangyaClient.Origin().
  public static POINT Origin(IntPtr h) {
    POINT p; p.X = 0; p.Y = 0;
    if (h != IntPtr.Zero) ClientToScreen(h, ref p);
    return p;
  }
}
"@ -ErrorAction SilentlyContinue

# Focus decides which instance the synthetic input reaches, and it cannot be TAKEN from a
# client that holds it — SetForegroundWindow (with and without SPI_SETFOREGROUNDLOCKTIMEOUT=0),
# absolute clicks on the other title bar or the taskbar, scan-code Alt+Tab / Alt+Esc, and
# SwitchToThisWindow all failed silently, because the focused client acquires its DirectInput
# devices exclusively: that suppresses the system task-switch keys and eats clicks aimed at any
# other window. Focus can only be GIVEN UP: minimising a DirectInput game forces it to
# unacquire, and minimising the foreground window releases the foreground lock, after which
# restoring another instance activates it and a single real title-bar click seals the
# activation, honoured by position now that nothing is grabbing input. The title bar and not
# the client area, because the engine acts on client-area clicks at wherever its own cursor
# froze, which would press a random widget.
#
# Do NOT reach for AttachThreadInput. Borrowing the input state of a busy game thread hangs the
# automation host and takes the whole PowerShell channel down with it for minutes. And never
# verify a switch with GetForegroundWindow: from this context it kept naming a window that had
# been minimised. A switch is proven by pixels, in Test-PangyaCursorFollows.
Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;using System.Threading;
public class PangyaActivate {
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx; public int dy; public uint mouseData; public uint dwFlags; public uint time; public IntPtr dwExtraInfo; }
  [StructLayout(LayoutKind.Explicit)] public struct INPUT { [FieldOffset(0)] public uint type; [FieldOffset(8)] public MOUSEINPUT mi; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }

  [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] p, int size);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);

  const uint LDOWN = 0x0002, LUP = 0x0004;

  public static bool IsAt(IntPtr h, int x, int y) {
    RECT r;
    if (!GetWindowRect(h, out r)) return false;
    return r.Left == x && r.Top == y;
  }

  // One absolute OS click well inside the title bar: past the icon, short of the caption
  // buttons. Only meaningful while no client holds input exclusively (i.e. all minimised).
  public static void TitleClick(IntPtr h) {
    RECT r;
    if (!GetWindowRect(h, out r)) return;
    SetCursorPos(r.Left + 120, r.Top + 14);
    Thread.Sleep(80);
    INPUT[] down = new INPUT[1]; down[0].type = 0; down[0].mi.dwFlags = LDOWN;
    INPUT[] up = new INPUT[1];   up[0].type = 0;   up[0].mi.dwFlags = LUP;
    SendInput(1, down, Marshal.SizeOf(typeof(INPUT)));
    Thread.Sleep(50);
    SendInput(1, up, Marshal.SizeOf(typeof(INPUT)));
    Thread.Sleep(200);
  }
}
"@ -ErrorAction SilentlyContinue

# The instance currently sitting at the origin, so a click does not re-anchor what is already
# anchored. See Confirm-PangyaAnchor.
$script:PangyaAnchoredPid = $null

function Get-PangyaInstances {
  Get-Process ProjectG -ErrorAction SilentlyContinue | ForEach-Object {
    $hwnds = [PangyaWindows]::Find([uint32]$_.Id)
    if ($hwnds.Count -gt 0) {
      [pscustomobject]@{ ProcessId = $_.Id; Hwnd = $hwnds[0]; Path = $_.Path }
    }
  }
}

# Proof that the instance behind $Hwnd is the one receiving input: the engine draws its own
# cursor at its accumulated position, so after a synthetic move the pixels around the probe
# point must change in step with where the cursor was sent — twice, so a lucky animation frame
# cannot pass for a cursor. The probe point must sit over STATIC UI of the target's current
# screen: the default is the lobby chat bar; the login dialog needs (325, 237). This never
# clicks, so it is safe to run against any screen.
function Test-PangyaCursorFollows {
  param(
    [Parameter(Mandatory=$true)]$Hwnd,
    [int]$ProbeX = 300, [int]$ProbeY = 545
  )
  Add-Type -AssemblyName System.Drawing
  $origin = [PangyaWindows]::Origin($Hwnd)
  $awayX = if ($ProbeX -gt 400) { $ProbeX - 250 } else { $ProbeX + 250 }
  $box = 26; $half = 13
  for ($round = 1; $round -le 2; $round++) {
    $samples = foreach ($cursorX in @($awayX, $ProbeX)) {
      [PangyaClient]::Goto($cursorX, $ProbeY)
      $bmp = New-Object System.Drawing.Bitmap $box, $box
      $g = [System.Drawing.Graphics]::FromImage($bmp)
      $g.CopyFromScreen($origin.X + $ProbeX - $half, $origin.Y + $ProbeY - $half, 0, 0, $bmp.Size)
      $g.Dispose()
      $bmp
    }
    $diff = 0
    for ($px = 0; $px -lt $box; $px++) {
      for ($py = 0; $py -lt $box; $py++) {
        $a = $samples[0].GetPixel($px, $py); $b = $samples[1].GetPixel($px, $py)
        if ([Math]::Abs($a.R - $b.R) -gt 25 -or [Math]::Abs($a.G - $b.G) -gt 25 -or
            [Math]::Abs($a.B - $b.B) -gt 25) { $diff++ }
      }
    }
    $samples | ForEach-Object { $_.Dispose() }
    if ($diff -lt 12) { return $false }
  }
  return $true
}

function Set-PangyaTarget {
  param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [int]$Attempts = 3,
    [int]$ProbeX = 300, [int]$ProbeY = 545
  )
  $all = @(Get-PangyaInstances)
  $target = $all | Where-Object ProcessId -eq $ProcessId
  if (-not $target) { throw "no client window for process $ProcessId" }
  for ($i = 1; $i -le $Attempts; $i++) {
    # Make whoever holds input let go: a minimised game unacquires its DirectInput devices,
    # and a minimised foreground window releases the foreground lock.
    foreach ($w in $all) { [PangyaWindows]::Minimize($w.Hwnd) }
    Start-Sleep -Milliseconds 800
    [PangyaWindows]::Restore($target.Hwnd)
    $deadline = (Get-Date).AddSeconds(10)
    while ((Get-Date) -lt $deadline -and [PangyaWindows]::IsMinimized($target.Hwnd)) {
      Start-Sleep -Milliseconds 200
    }
    Start-Sleep -Milliseconds 500
    # At the origin, verified: the restore animation swallows a move issued too early, and a
    # click on a window sitting anywhere else lands outside it (constraint 2).
    $atOrigin = $false
    for ($m = 0; $m -lt 5; $m++) {
      [PangyaWindows]::Move($target.Hwnd, 0, 0)
      Start-Sleep -Milliseconds 400
      if ([PangyaActivate]::IsAt($target.Hwnd, 0, 0)) { $atOrigin = $true; break }
    }
    if (-not $atOrigin) { continue }
    [PangyaActivate]::TitleClick($target.Hwnd)
    Start-Sleep -Milliseconds 700
    if (Test-PangyaCursorFollows -Hwnd $target.Hwnd -ProbeX $ProbeX -ProbeY $ProbeY) {
      $script:PangyaTargetPid = $ProcessId
      $script:PangyaAnchoredPid = $ProcessId
      return $target
    }
  }
  throw ("client $ProcessId never took input focus: its engine cursor ignored the probe move " +
         "on $Attempts attempt(s); a click now could drive another instance")
}

function Get-PangyaTarget { $script:PangyaTargetPid }

# Re-asserts the target window's position before a click, instead of PangyaClient.Anchor().
#
# The full switch costs seconds and disturbs every window, so this re-runs it only when the
# instance being driven has changed since the last anchor, which is why a run of clicks on one
# client — typing an id, then a password, then pressing Login — is not interrupted by window
# churn. Pass -Force after anything that could have moved a window out from under us.
function Confirm-PangyaAnchor {
  param([switch]$Force)
  if (-not $script:PangyaTargetPid) {
    [PangyaClient]::Anchor() | Out-Null
    return
  }
  if (-not $Force -and $script:PangyaAnchoredPid -eq $script:PangyaTargetPid) { return }
  Set-PangyaTarget -ProcessId $script:PangyaTargetPid | Out-Null
}

# Client-area origin of the instance being driven. Falls back to the single-window lookup when
# no target is set. Every screen probe below reads through this.
function Get-PangyaOrigin {
  if ($script:PangyaTargetPid) {
    $hwnds = [PangyaWindows]::Find([uint32]$script:PangyaTargetPid)
    if ($hwnds.Count -gt 0) { return [PangyaWindows]::Origin($hwnds[0]) }
  }
  return [PangyaClient]::Origin()
}

# ---- Input --------------------------------------------------------------------------------
# Every click clears a pending notice first. A stray click on the player-list pane opens one,
# and while it is up it swallows every later click, which makes the whole run look like a server
# fault. -SkipNoticeCheck exists so the dismissal's own click does not recurse.
function Invoke-PangyaClick {
  param([int]$X, [int]$Y, [switch]$SkipNoticeCheck)
  if (-not $SkipNoticeCheck) { Dismiss-PangyaNotice | Out-Null }
  Confirm-PangyaAnchor
  [PangyaClient]::Click($X, $Y)
}

function Invoke-PangyaDoubleClick {
  param([int]$X, [int]$Y, [switch]$SkipNoticeCheck)
  if (-not $SkipNoticeCheck) { Dismiss-PangyaNotice | Out-Null }
  Confirm-PangyaAnchor
  [PangyaClient]::DoubleClick($X, $Y)
}

function Send-PangyaText { param([string]$Text) [PangyaClient]::Text($Text) }

# ---- Screen recognition -------------------------------------------------------------------
# Waits are on observed UI state, not fixed sleeps. A sleep is either longer than it needs to be
# or too short on a slow frame; polling for the thing being waited on is faster and does not fail
# intermittently. Every region below was chosen by eye against a screenshot and is worth
# re-verifying if a wait starts passing or failing wrongly.
function Wait-PangyaText {
  param(
    [int]$X, [int]$Y, [int]$Width, [int]$Height,
    [int]$TimeoutSeconds = 45,
    [int]$Threshold = 110
  )
  Add-Type -AssemblyName System.Drawing
  $origin = Get-PangyaOrigin
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $deadline) {
    $bmp = New-Object System.Drawing.Bitmap $Width, $Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($origin.X + $X, $origin.Y + $Y, 0, 0, $bmp.Size)
    $dark = 0
    for ($px = 0; $px -lt $Width; $px += 2) {
      for ($py = 0; $py -lt $Height; $py += 2) {
        $c = $bmp.GetPixel($px, $py)
        if ((($c.R + $c.G + $c.B) / 3) -lt $Threshold) { $dark++ }
      }
    }
    $g.Dispose(); $bmp.Dispose()
    if ($dark -ge 6) { return $true }
    Start-Sleep -Milliseconds 700
  }
  return $false
}

# Regions, in client-area pixels, that identify a screen by the text drawn in it.
$script:PangyaRegions = @{
  LoginDialog = @{ X = 280; Y = 230; W = 90;  H = 14 }   # the "Password" label
  NickDialog  = @{ X = 268; Y = 210; W = 110; H = 14 }   # "Enter nick name:"
  ServerList  = @{ X = 228; Y = 183; W = 100; H = 16 }   # the first server row
  ChannelList = @{ X = 470; Y = 215; W = 90;  H = 16 }   # the first channel row
  LobbyBar    = @{ X = 80;  Y = 580; W = 120; H = 20 }   # the bottom menu bar
  RoomDirectory = @{ X = 562; Y = 92; W = 150; H = 14 }  # "M/F Level Guild" column header
}

function Wait-PangyaScreen {
  param([Parameter(Mandatory=$true)][string]$Name, [int]$TimeoutSeconds = 45)
  $r = $script:PangyaRegions[$Name]
  if (-not $r) { throw "unknown screen '$Name'" }
  Wait-PangyaText -X $r.X -Y $r.Y -Width $r.W -Height $r.H -TimeoutSeconds $TimeoutSeconds
}

# Returns the first of several screens to appear, or $null. Lets a caller branch on whichever
# way the client went instead of sleeping long enough for the slowest path.
function Wait-PangyaAnyScreen {
  param([Parameter(Mandatory=$true)][string[]]$Names, [int]$TimeoutSeconds = 45)
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $deadline) {
    foreach ($name in $Names) {
      if (Wait-PangyaScreen -Name $name -TimeoutSeconds 1) { return $name }
    }
  }
  return $null
}

function Wait-PangyaServerList { Wait-PangyaScreen -Name ServerList }
function Wait-PangyaChannelList { Wait-PangyaScreen -Name ChannelList }

# The client reports a failed server entry as a red "Server is full." label at the bottom-left of
# the Select Server dialog. It blinks, so a single sample can miss it; this watches for a while.
# Detecting it explicitly matters because the same message covers two unrelated causes: a
# malformed server list, and clicking a list row before the list has rendered.
function Test-PangyaServerFull {
  param([double]$WatchSeconds = 3.0)
  Add-Type -AssemblyName System.Drawing
  $origin = Get-PangyaOrigin
  $x = 175; $y = 488; $w = 120; $h = 18
  $deadline = (Get-Date).AddSeconds($WatchSeconds)
  while ((Get-Date) -lt $deadline) {
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($origin.X + $x, $origin.Y + $y, 0, 0, $bmp.Size)
    $red = 0
    for ($px = 0; $px -lt $w; $px++) {
      for ($py = 0; $py -lt $h; $py++) {
        $c = $bmp.GetPixel($px, $py)
        # The label is saturated red on a near-white panel; ordinary UI text is grey.
        if ($c.R -gt 120 -and $c.G -lt 90 -and $c.B -lt 90 -and ($c.R - $c.G) -gt 60) { $red++ }
      }
    }
    $g.Dispose(); $bmp.Dispose()
    if ($red -ge 8) { return $true }
    Start-Sleep -Milliseconds 250
  }
  return $false
}

function Assert-PangyaNotServerFull {
  if (Test-PangyaServerFull) {
    throw 'client reports "Server is full." - the selected row was empty, or the server list is malformed'
  }
}

# A mis-aimed click on the player-list pane opens a "You have left a message" notice, which then
# swallows every later click. Detect it by its blue title strip and clear it before continuing.
function Test-PangyaNotice {
  Add-Type -AssemblyName System.Drawing
  $origin = Get-PangyaOrigin
  $x = 255; $y = 185; $w = 260; $h = 8
  $bmp = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($origin.X + $x, $origin.Y + $y, 0, 0, $bmp.Size)
  $blue = 0
  for ($px = 0; $px -lt $w; $px += 2) {
    for ($py = 0; $py -lt $h; $py++) {
      $c = $bmp.GetPixel($px, $py)
      if ($c.B -gt 140 -and ($c.B - $c.R) -gt 40) { $blue++ }
    }
  }
  $g.Dispose(); $bmp.Dispose()
  return ($blue -ge 40)
}

function Dismiss-PangyaNotice {
  param([int]$Attempts = 3)
  for ($i = 0; $i -lt $Attempts; $i++) {
    if (-not (Test-PangyaNotice)) { return $true }
    Invoke-PangyaClick 393 361 -SkipNoticeCheck
    Start-Sleep -Milliseconds 600
  }
  return (-not (Test-PangyaNotice))
}

# ---- Verified steps -----------------------------------------------------------------------
# A step is a click plus the screen it is supposed to produce, retried until it appears or until
# it gives up naming what it was waiting for.
function Invoke-PangyaStep {
  param(
    [Parameter(Mandatory=$true)][int]$X,
    [Parameter(Mandatory=$true)][int]$Y,
    [Parameter(Mandatory=$true)][string]$Until,
    [int]$Attempts = 4,
    [int]$TimeoutSeconds = 10,
    [switch]$DoubleClick,
    [string]$What
  )
  if (-not $What) { $What = "click ($X,$Y)" }
  for ($i = 1; $i -le $Attempts; $i++) {
    if ($DoubleClick) { Invoke-PangyaDoubleClick $X $Y } else { Invoke-PangyaClick $X $Y }
    if (Wait-PangyaScreen -Name $Until -TimeoutSeconds $TimeoutSeconds) { return $true }
    if ($Until -eq 'ChannelList' -and (Test-PangyaServerFull -WatchSeconds 1)) {
      Write-Warning "$What produced 'Server is full.' on attempt $i; the row was probably not drawn yet"
    }
  }
  throw "$What never reached '$Until' after $Attempts attempts"
}

# The same, for places where a single click is not the interaction.
function Invoke-PangyaVerified {
  param(
    [Parameter(Mandatory=$true)][scriptblock]$Action,
    [Parameter(Mandatory=$true)][string]$Until,
    [int]$Attempts = 4,
    [int]$TimeoutSeconds = 10,
    [string]$What = 'step'
  )
  for ($i = 1; $i -le $Attempts; $i++) {
    & $Action
    if (Wait-PangyaScreen -Name $Until -TimeoutSeconds $TimeoutSeconds) { return $true }
  }
  throw "$What never reached '$Until' after $Attempts attempts"
}

# ---- Widget positions ---------------------------------------------------------------------
$script:LoginIdField   = @(401,208)
$script:LoginPwField   = @(401,235)
$script:LoginButton    = @(503,219)
$script:CharConfirm    = @(392,531)
$script:CharConfirmYes = @(337,358)
$script:ServerFirstRow = @(270,190)
$script:ChannelFirstRow = @(494,222)
$script:NickField      = @(407,217)
$script:NickConfirm    = @(528,217)
$script:NickYes        = @(353,371)
$script:ShopButton      = @(224,565)
$script:ShopFirstCart   = @(556,224)
$script:ShopBuyConfirm  = @(351,502)
$script:ShopSureConfirm = @(351,363)
$script:ShopTabs = @{
  'Clothes' = @(483,96); 'Item' = @(530,96)
  'Accessories' = @(626,130); 'ClubSets' = @(425,130)
  'Comet' = @(482,130); 'ActiveItems' = @(544,130)
}

# ---- Flows --------------------------------------------------------------------------------
# The password field is the one place a swallowed click is invisible: the field masks its
# content, so "typed nothing" and "typed the password" look identical, and the Login button then
# does nothing at all. Both fields are therefore filled and checked before Login is pressed.
function Invoke-PangyaLogin {
  param(
    [Parameter(Mandatory=$true)][string]$Id,
    [Parameter(Mandatory=$true)][string]$Password,
    [int]$Attempts = 3
  )
  for ($i = 1; $i -le $Attempts; $i++) {
    Invoke-PangyaClick @script:LoginIdField
    Send-PangyaText $Id
    Invoke-PangyaClick @script:LoginPwField
    Send-PangyaText $Password
    if (Test-PangyaLoginFilled) {
      Invoke-PangyaClick @script:LoginButton
      return $true
    }
    # Clear whatever did land, so a retry does not append to a half-typed field.
    Invoke-PangyaClick @script:LoginIdField
    Send-PangyaText ([string][char]8 * 24)
    Invoke-PangyaClick @script:LoginPwField
    Send-PangyaText ([string][char]8 * 24)
  }
  throw 'login fields never both filled'
}

# Both login fields draw dark glyphs on a white field, so "has something in it" is a dark-pixel
# count in each field's own rectangle. Cheaper and more reliable than reading the text back.
function Test-PangyaLoginFilled {
  Add-Type -AssemblyName System.Drawing
  $origin = Get-PangyaOrigin
  foreach ($box in @(@{X=356;Y=202;W=90;H=13}, @{X=356;Y=229;W=90;H=13})) {
    $bmp = New-Object System.Drawing.Bitmap $box.W, $box.H
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($origin.X + $box.X, $origin.Y + $box.Y, 0, 0, $bmp.Size)
    $dark = 0
    for ($px = 0; $px -lt $box.W; $px++) {
      for ($py = 0; $py -lt $box.H; $py++) {
        $c = $bmp.GetPixel($px, $py)
        if ((($c.R + $c.G + $c.B) / 3) -lt 110) { $dark++ }
      }
    }
    $g.Dispose(); $bmp.Dispose()
    if ($dark -lt 8) { return $false }
  }
  return $true
}

function Set-PangyaNickname {
  param([Parameter(Mandatory=$true)][string]$Nickname)
  Invoke-PangyaClick @script:NickField
  Send-PangyaText $Nickname
  Invoke-PangyaClick @script:NickConfirm   # sends 0x0007 check
  Start-Sleep -Seconds 2
  Invoke-PangyaClick @script:NickYes       # sends 0x0006 set
}

# Only shown the first time an account logs in, and only when the server asks for a character
# rather than a nickname.
function Confirm-PangyaCharacter {
  Invoke-PangyaClick @script:CharConfirm
  Start-Sleep -Seconds 2
  Invoke-PangyaClick @script:CharConfirmYes
}

# A row must be selected before a double click opens it; a bare double click only highlights it.
function Select-PangyaServer {
  Invoke-PangyaClick @script:ServerFirstRow
  Start-Sleep -Milliseconds 600
  Invoke-PangyaDoubleClick @script:ServerFirstRow
}

function Enter-PangyaChannel {
  param([int]$Attempts = 4)
  if (-not (Wait-PangyaScreen -Name ServerList -TimeoutSeconds 45)) {
    Assert-PangyaNotServerFull
    throw 'server list did not render'
  }
  Invoke-PangyaVerified -Action { Select-PangyaServer } -Until ChannelList -Attempts $Attempts `
    -What 'server selection' | Out-Null
  Invoke-PangyaVerified -Action { Invoke-PangyaDoubleClick @script:ChannelFirstRow } `
    -Until LobbyBar -Attempts $Attempts -TimeoutSeconds 15 -What 'channel entry' | Out-Null
}

# Opens the retail shop and proves that its top-level tabs rendered. This replaces the raw click
# sequence that was first learned while validating authored IFF data.
function Open-PangyaShop {
  Invoke-PangyaClick @script:ShopButton
  if (-not (Wait-PangyaText -X 405 -Y 88 -Width 145 -Height 18 -TimeoutSeconds 15)) {
    throw 'shop tabs did not render'
  }
}

# Selects named tabs rather than repeating coordinates in every evidence run. Top is optional so
# callers can move between siblings after the shop is already open.
function Select-PangyaShopCategory {
  param(
    [ValidateSet('Clothes','Item')][string]$Top,
    [ValidateSet('Accessories','ClubSets','Comet','ActiveItems')][string]$Sub
  )
  if ($Top) {
    $position = $script:ShopTabs[$Top]
    Invoke-PangyaClick @position
    Start-Sleep -Milliseconds 700
  }
  if ($Sub) {
    $position = $script:ShopTabs[$Sub]
    Invoke-PangyaClick @position
    Start-Sleep -Milliseconds 700
  }
}

# Purchases the first visible shop cell. The U.S. 852 client requires TWO distinct confirmation
# clicks: the Buy Item form and then its nested "Are you sure" Notice. The second click bypasses
# automatic notice dismissal deliberately; otherwise Invoke-PangyaClick would consume the modal
# before the intended coordinate is pressed. The final success/refusal Notice remains open for a
# screenshot unless -DismissResult is requested.
function Invoke-PangyaShopFirstItemPurchase {
  param([switch]$DismissResult)
  Invoke-PangyaClick @script:ShopFirstCart
  if (-not (Wait-PangyaText -X 250 -Y 105 -Width 190 -Height 70 -TimeoutSeconds 10)) {
    throw 'Buy Item form did not render'
  }
  Invoke-PangyaClick @script:ShopBuyConfirm
  $deadline = (Get-Date).AddSeconds(10)
  while ((Get-Date) -lt $deadline -and -not (Test-PangyaNotice)) {
    Start-Sleep -Milliseconds 250
  }
  if (-not (Test-PangyaNotice)) { throw 'purchase confirmation Notice did not render' }
  Invoke-PangyaClick @script:ShopSureConfirm -SkipNoticeCheck
  Start-Sleep -Seconds 3
  if (-not (Test-PangyaNotice)) { throw 'purchase result Notice did not render' }
  if ($DismissResult) { Dismiss-PangyaNotice | Out-Null }
}

# ---- Instance lifecycle -------------------------------------------------------------------
function Stop-AllPangyaClients {
  Get-Process ProjectG -ErrorAction SilentlyContinue | Stop-Process -Force
  $deadline = (Get-Date).AddSeconds(15)
  while ((Get-Date) -lt $deadline -and (Get-Process ProjectG -ErrorAction SilentlyContinue)) {
    Start-Sleep -Milliseconds 300
  }
  $script:PangyaTargetPid = $null
  $script:PangyaAnchoredPid = $null
}

# Launches one install without disturbing the others, identifying the new process by diffing the
# process list rather than guessing. Running two instances needs the Rugburn
# AllowMultipleInstances patch; see docs/RUNNING_THE_CLIENT.md.
function Start-PangyaClientAt {
  param([Parameter(Mandatory=$true)][string]$Path, [int]$TimeoutSeconds = 120)
  $before = @(Get-Process ProjectG -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
  Start-Process -FilePath (Join-Path $Path 'ProjectG.exe') -WorkingDirectory $Path
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  $pidNew = $null
  while (-not $pidNew -and (Get-Date) -lt $deadline) {
    $now = @(Get-Process ProjectG -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id)
    $pidNew = ($now | Where-Object { $_ -notin $before }) | Select-Object -First 1
    if (-not $pidNew) { Start-Sleep -Milliseconds 500 }
  }
  if (-not $pidNew) { throw "no new client process appeared for $Path" }
  # The process exists before its window does, so wait for the window before targeting it.
  $windowDeadline = (Get-Date).AddSeconds(60)
  while ((Get-Date) -lt $windowDeadline -and
         ([PangyaWindows]::Find([uint32]$pidNew)).Count -eq 0) {
    Start-Sleep -Milliseconds 500
  }
  if (([PangyaWindows]::Find([uint32]$pidNew)).Count -eq 0) {
    throw "client $pidNew never created a window"
  }
  # A window created a moment ago holds focus naturally — Windows grants it to new processes —
  # so the full switch is skipped: it would even mis-verify here, because the client draws no
  # cursor until its UI is up. Minimise the others and take the origin directly.
  $hwndNew = ([PangyaWindows]::Find([uint32]$pidNew))[0]
  foreach ($w in @(Get-PangyaInstances) | Where-Object ProcessId -ne $pidNew) {
    [PangyaWindows]::Minimize($w.Hwnd)
  }
  Start-Sleep -Milliseconds 500
  for ($m = 0; $m -lt 5; $m++) {
    [PangyaWindows]::Move($hwndNew, 0, 0)
    Start-Sleep -Milliseconds 400
    if ([PangyaActivate]::IsAt($hwndNew, 0, 0)) { break }
  }
  $script:PangyaTargetPid = [int]$pidNew
  $script:PangyaAnchoredPid = [int]$pidNew
  # The window then exists long before the login dialog: the client fetches its updatelist and
  # theme over HTTP first. Wait for the dialog rather than guessing how long that takes.
  if (-not (Wait-PangyaScreen -Name LoginDialog -TimeoutSeconds $TimeoutSeconds)) {
    throw "client $pidNew never reached its login dialog"
  }
  # Natural focus is still an assumption; prove it now that the dialog (and with it the engine
  # cursor) exists, and fall back to the full switch if some other window stole it meanwhile.
  if (-not (Test-PangyaCursorFollows -Hwnd $hwndNew -ProbeX 325 -ProbeY 237)) {
    Set-PangyaTarget -ProcessId ([int]$pidNew) -ProbeX 325 -ProbeY 237 | Out-Null
  }
  return [int]$pidNew
}

# Kept for single-instance use: kills every client, launches the default install, anchors it.
function Start-PangyaClient {
  param([string]$Path = 'C:\pangya\us851', [int]$TimeoutSeconds = 120)
  Stop-AllPangyaClients
  $id = Start-PangyaClientAt -Path $Path -TimeoutSeconds $TimeoutSeconds
  $o = Get-PangyaOrigin
  [pscustomobject]@{ Ready = $true; ProcessId = $id; OriginX = $o.X; OriginY = $o.Y }
}

# Signs one instance in and leaves it in the lobby, branching on whichever screen the client
# actually shows rather than assuming a first or a returning login.
function Invoke-PangyaSignIn {
  param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [Parameter(Mandatory=$true)][string]$Id,
    [Parameter(Mandatory=$true)][string]$Password,
    [string]$Nickname
  )
  # The client shows its login dialog here, so the switch is verified against the dialog's
  # static pixels — the lobby chat bar it probes by default does not exist yet.
  Set-PangyaTarget -ProcessId $ProcessId -ProbeX 325 -ProbeY 237 | Out-Null
  Invoke-PangyaLogin -Id $Id -Password $Password
  $screen = Wait-PangyaAnyScreen -Names @('ServerList', 'NickDialog') -TimeoutSeconds 45
  if (-not $screen) { throw "client $ProcessId showed neither a server list nor a nickname prompt" }
  if ($screen -eq 'NickDialog') {
    if (-not $Nickname) { throw "client $ProcessId needs a nickname but none was given" }
    Set-PangyaNickname -Nickname $Nickname
    Invoke-PangyaClick @script:NickYes
    if (-not (Wait-PangyaScreen -Name ServerList -TimeoutSeconds 45)) {
      throw "client $ProcessId did not reach the server list after setting a nickname"
    }
  }
  Enter-PangyaChannel
  $ProcessId
}
