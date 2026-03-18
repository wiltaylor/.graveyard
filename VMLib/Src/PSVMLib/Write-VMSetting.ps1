<# 
.SYNOPSIS
	Writes a setting to virtual machine.
.DESCRIPTION
	This cmdlet can write 3 different types of setting.
.PARAMETER Type
	Can be set to one of the following
	Environment - Writes environment variables.
	VMSetting - Writes a virtual machine setting.
	GuestSetting - Writes a guest setting. This can be retrived by scripts on both the host and virtual machine.
.PARAMETER Name
	Name of the setting to set.
.PARAMETER Value
	Value to set setting to.
.EXAMPLE
	Sets mysetting to foo
	$vm, | Write-VMSetting -Type GuestSetting -Name MySetting -Value Foo
.INPUTS
	IVirtualMachine
#> 
[CmdletBinding]
function Write-VMSetting{
		param([Parameter(Mandatory = $true, ValueFromPipeline = $true)]$vm, 
	      [Parameter(Mandatory=$true)][ValidateSet("Environment", "VMSetting", "GuestSetting")]$Type,
		  [Parameter(Mandatory=$true)][string]$Name,
		  [Parameter(Mandatory=$true)][string]$Value)

		  if($Type -eq "Environment")
		  {
			$vm.WriteEnvironment($Name, $Value)
		  }

		  if($Type -eq "GuestSetting")
		  {
			$vm.WriteGuestVariable($Name, $Value)
		  }

		  if($Type -eq "VMSetting")
		  {
			$vm.WriteVMSetting($Name, $Value)
		  }

}