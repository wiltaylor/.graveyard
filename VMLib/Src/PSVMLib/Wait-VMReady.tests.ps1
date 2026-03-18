.  $PSScriptRoot\Wait-VMReady.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Wait-VMReady" {
	Context "Able to wait for VM to be ready" {
		$script:ReadyFlag = $false
		$script:OffFlag = $false
		$fakevm = New-FakeVM -WaitTillReady {
			$script:ReadyFlag = $true

		} -WaitTillOff {
			$script:OffFlag = $true
		}

		It "Can wait till powered off." {
			$fakevm | Wait-VMReady -Off
			$script:OffFlag | should be $true
		}

		It "Can wait till ready." {
			$fakevm | Wait-VMReady
			$script:ReadyFlag | should be $true
		}
	}
}