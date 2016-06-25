<# 
.SYNOPSIS
	Tests if a file or directory exists in VM.
.DESCRIPTION
	Tests if a file or directory exists in VM.
.PARAMETER Path
	Path of item to test
.PARAMETER Type
	Can be either File or Directory.
.EXAMPLE
	Tests if file exists on VM.
	if($vm | Test-ItemInVM -Path c:\guestpath\file.txt -Type File) { Write-Host 'Yeah  it exists!' }
.INPUTS
	IVMSnapshot
#> 
[CmdletBinding]
function Test-ItemInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][string]$Path,
		  [Parameter(Mandatory=$true)][ValidateSet('File', 'Directory')][string]$Type)

	Process 
	{
		if($Type -eq "File") { $vm.FileExists($path)}
		if($Type -eq "Directory") { $vm.DirectoryExists($path)}
	}
}