<# 
.SYNOPSIS
	Copys a file from VM guest operating system to host operating system.
.DESCRIPTION
	Copies target file from the guest into target location on the host operating system.
.PARAMETER Source
	Location on guest to copy.
.PARAMETER Destination
	Location to copy file to on the host operating system.
.EXAMPLE
	Copy myutil from guest operating system
	$vm, | Copy-FileFromVM -Source c:\guestpath\myutil.exe -Destination c:\hostpath\myutil.exe
.INPUTS
	IVMSnapshot
#> 
[CmdletBinding]
function Copy-FileFromVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][string]$Source,
		  [Parameter(Mandatory=$true)][string]$Destination)

	Process 
	{
		$vm.CopyFromVM($Source, $Destination)
	}
}