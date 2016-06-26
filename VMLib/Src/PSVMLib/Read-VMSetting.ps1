<# 
.SYNOPSIS
	Reads a setting to virtual machine.
.DESCRIPTION
	This cmdlet can read 3 different types of setting.
.PARAMETER Type
	Can be set to one of the following
	Environment - Reads environment variables.
	VMSetting - Reads a virtual machine setting.
	GuestSetting - Reads a guest setting. This can be retrived by scripts on both the host and virtual machine.
.PARAMETER Name
	Name of the setting to get.
.EXAMPLE
	Get mysetting to foo
	$foo = $vm | Read-VMSetting -Type GuestSetting -Name foo 
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Read-VMSetting{
		param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][ValidateSet("Environment", "VMSetting", "GuestSetting")]$Type,
		  [Parameter(Mandatory=$true)][string]$Name)

		  if($Type -eq "Environment")
		  {
			$vm.ReadEnvironment($Name)
		  }

		  if($Type -eq "GuestSetting")
		  {
			$vm.ReadGuestVariable($Name)
		  }

		  if($Type -eq "VMSetting")
		  {
			$vm.ReadVMSetting($Name)
		  }

}