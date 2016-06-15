<# 
.SYNOPSIS
	Copys a file into VM guest operating system.
.DESCRIPTION
	Copies target file from the host into target location inside the guest operating system.
.PARAMETER Source
	Location on host to copy.
.PARAMETER Destination
	Location to copy file to in the guest operating system.
.EXAMPLE
	Copy myutil into guest operating system
	$vm, | Copy-FileToVM -Source c:\tools\myutil.exe -Destination c:\guestpath\myutil.exe
.INPUTS
	IVMSnapshot
#> 
[CmdletBinding]
function Copy-FileToVM
{
	param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][string]$Source,
		  [Parameter(Mandatory=$true)][string]$Destination)

	Process 
	{

	}
}