.  $PSScriptRoot\New-HypervisorInfo.ps1
.  $PSScriptRoot\Fakes\Fake_HypervisorFactory.ps1
.  $PSScriptRoot\Fakes\Fake_HypervisorConnectionInfo.ps1

Describe "New-HypervisorInfo" {
	Context "Creating hypervisor object for existing hypervisor" {
		$script:HypervisorFactory = New-FakeHypervisorFactory -CreateHypervisorConnectionInfo { 
			param($Name) 
			New-FakeHypervisorConnectionInfo -Name $Name
		}
				
		It "Calling function returns an object" {
			$result = New-HypervisorInfo -Name "Test"
			$result.Name | should be "Test"
		}
	}
}