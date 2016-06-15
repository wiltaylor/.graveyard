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
	Copy myutil from guest operating system
	$vm | Rename-ItemInVM -Path c:\guestpath\file.txt -Type File
.INPUTS
	IVMSnapshot
#> 
[CmdletBinding]
function Rename-ItemInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][string]$Path,
		  [Parameter(Mandatory=$true)][ValidateSet('File', 'Directory')][string]$Type)

	Process 
	{

	}
}