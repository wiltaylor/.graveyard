<# 
.SYNOPSIS
	Remove file or folder from vm.
.DESCRIPTION
	Remove target file or folder from vm.
.PARAMETER Path
	Path of item to remove.
.PARAMETER Type
	Can be either File or Directory.
.EXAMPLE
	Remove myutil from guest operating system
	$vm | Remove-ItemInVM -Path c:\guestpath\file.txt -Type File
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Remove-ItemInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][string]$Path,
		  [Parameter(Mandatory=$true)][ValidateSet('File', 'Directory')][string]$Type)

	Process 
	{
		if($Type -eq "File") { $vm.DeleteFile($Path)}
		if($Type -eq "Directory") { $vm.DeleteDirectory($Path)}
	}
}