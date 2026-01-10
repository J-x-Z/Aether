# Aether Hyper-V Setup Script
# Run this script as Administrator

$VmName = "Aether-Debug"
$VhdPath = "C:\Users\Z1529\.gemini\antigravity\scratch\Aether\aether-boot.vhdx"
$EspPath = "C:\Users\Z1529\.gemini\antigravity\scratch\Aether\esp"

# Check if VM exists
$existingVm = Get-VM -Name $VmName -ErrorAction SilentlyContinue
if ($existingVm) {
    Write-Host "Stopping existing VM..."
    Stop-VM -Name $VmName -Force -ErrorAction SilentlyContinue
    Write-Host "Removing existing VM..."
    Remove-VM -Name $VmName -Force
}

# Remove old VHD
if (Test-Path $VhdPath) {
    Remove-Item $VhdPath -Force
}

# Create new VHD (FAT32 formatted with EFI files)
Write-Host "Creating VHD..."
New-VHD -Path $VhdPath -SizeBytes 512MB -Dynamic

# Mount and format VHD
Write-Host "Mounting VHD..."
$vhd = Mount-VHD -Path $VhdPath -Passthru
$disk = Get-Disk -Number $vhd.DiskNumber

# Initialize and partition
Write-Host "Initializing disk..."
Initialize-Disk -Number $disk.Number -PartitionStyle GPT

# Create EFI System Partition
Write-Host "Creating EFI partition..."
$partition = New-Partition -DiskNumber $disk.Number -UseMaximumSize -GptType "{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}"
$partition | Format-Volume -FileSystem FAT32 -NewFileSystemLabel "EFI" -Confirm:$false

# Assign drive letter
$driveLetter = $partition | Add-PartitionAccessPath -AssignDriveLetter -PassThru | Get-Partition | Select-Object -ExpandProperty DriveLetter

Write-Host "Copying EFI files to $driveLetter`:..."
Copy-Item -Path "$EspPath\*" -Destination "$driveLetter`:\" -Recurse -Force

# Dismount VHD
Write-Host "Dismounting VHD..."
Dismount-VHD -Path $VhdPath

# Create Gen2 VM with UEFI
Write-Host "Creating Hyper-V VM..."
New-VM -Name $VmName -Generation 2 -MemoryStartupBytes 512MB -VHDPath $VhdPath

# Configure VM
Set-VMFirmware -VMName $VmName -EnableSecureBoot Off
Set-VMProcessor -VMName $VmName -Count 2

# Enable COM1 for serial output
Set-VMComPort -VMName $VmName -Number 1 -Path "\\.\pipe\aether-serial"

Write-Host "VM '$VmName' created successfully!"
Write-Host "To start: Start-VM -Name '$VmName'"
Write-Host "To connect: vmconnect localhost '$VmName'"
Write-Host "Serial output available at: \\.\pipe\aether-serial"
