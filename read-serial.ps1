# Read serial output from Hyper-V VM
$pipeName = "aether-serial"
$pipePath = "\\.\pipe\$pipeName"

Write-Host "Connecting to serial pipe: $pipePath"
Write-Host "Waiting for VM boot output..."
Write-Host "-------------------------------------------"

try {
    $pipe = New-Object System.IO.Pipes.NamedPipeClientStream(".", $pipeName, [System.IO.Pipes.PipeDirection]::In)
    $pipe.Connect(5000)  # 5 second timeout
    
    $reader = New-Object System.IO.StreamReader($pipe)
    $startTime = Get-Date
    $timeout = 30  # 30 seconds timeout
    
    while ($true) {
        if ($reader.Peek() -ge 0) {
            $line = $reader.ReadLine()
            Write-Host $line
        }
        
        # Check timeout
        $elapsed = (Get-Date) - $startTime
        if ($elapsed.TotalSeconds -gt $timeout) {
            Write-Host "-------------------------------------------"
            Write-Host "Timeout reached ($timeout seconds)"
            break
        }
        
        Start-Sleep -Milliseconds 100
    }
    
    $reader.Close()
    $pipe.Close()
} catch {
    Write-Host "Error: $_"
    Write-Host "The VM may not have serial output enabled or hasn't started yet."
}
