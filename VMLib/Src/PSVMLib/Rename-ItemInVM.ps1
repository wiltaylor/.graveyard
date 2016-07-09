<# 
.SYNOPSIS
	Rename file or folder from vm.
.DESCRIPTION
	Rename target file or folder from vm.
.PARAMETER Path
	Path of item to be renamed.
.PARAMETER NewName
	New name of item being renamed.
.PARAMETER Type
	Can be either File or Directory.
.EXAMPLE
	Copy myutil from guest operating system
	$vm, | Rename-ItemInVM -Path c:\guestpath\file.txt -NewName c:\guestpath\newname.txt -Type File
.INPUTS
	IVMSnapshot
#> 
[CmdletBinding]
function Rename-ItemInVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][string]$Path,
		  [Parameter(Mandatory=$true)][string]$NewName,
		  [Parameter(Mandatory=$true)][ValidateSet('File', 'Directory')][string]$Type)

	Process 
	{
		switch($Type)
		{
			"File" { $vm.RenameFile($Path, $NewName) }
			"Directory" { $vm.RenameDirectory($Path, $NewName) }
		}
	}
}