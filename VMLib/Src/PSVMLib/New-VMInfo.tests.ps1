.  $PSScriptRoot\New-VMInfo.ps1
.  $PSScriptRoot\Fakes\Fake_Hardware.ps1

Describe "New-VMInfo" {
	Context "New VMInfo Creation" {
		It "Can create new VMInfo" {
			New-VMInfo | should not be $null
		}
	}
}