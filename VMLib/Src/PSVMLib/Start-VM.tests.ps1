.  $PSScriptRoot\Start-VM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Start-VM" {
	Context "Can start virtual machine" {
		$script:TestFlag = $false	
		$fakevm = New-FakeVM -Start {
			$script:TestFlag = $true	
		}
		It "starts virtual machine" {
			$fakevm | Start-VM
			$script:TestFlag | should be $true
		}
	}
}