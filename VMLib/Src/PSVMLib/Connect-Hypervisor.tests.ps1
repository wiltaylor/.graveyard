.  $PSScriptRoot\Connect-Hypervisor.ps1
.  $PSScriptRoot\Fakes\Fake_HypervisorFactory.ps1
.  $PSScriptRoot\Fakes\Fake_HypervisorConnectionInfo.ps1
.  $PSScriptRoot\Fakes\Fake_Hypervisor.ps1

Describe "Connect-Hypervisor" {
	Context "Can create connection" {
		$script:HypervisorFactory = New-FakeHypervisorFactory -CreateHypervisor { 
			param($Name, $InfoObject) 
			New-FakeHypervisor -Name $Name
		}

		$fakeinfo = New-FakeHypervisorConnectionInfo -Name "FakeHypervisor"

		It "Calling function returns object" {
			Connect-Hypervisor -Name "FakeHypervisor" -Settings $fakeinfo | should not be $null
		}
	}
}