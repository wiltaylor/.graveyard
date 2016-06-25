.  $PSScriptRoot\Stop-VM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Stop-VM" {
	Context "can stop vm" {
		$script:TestFlag = $false	
		$fakevm = New-FakeVM -Stop {
			param($force)
			$script:TestFlag = $true

		}
		It "stops vm" {
			$fakevm | Stop-VM
			$script:TestFlag | should be $true

		}
	}
}