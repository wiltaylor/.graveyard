.  $PSScriptRoot\Remove-VMSnapshot.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1


Describe "Remove-VMSnapshot" {
	Context "Can remove snapshot" {
		$script:TestFlag = $false
		$fakevm = New-FakeVM -RemoveSnapshot {		
			param($name)

			if($name -eq "MySnapshot") {
				$script:TestFlag = $true
			}
		}

		It "Removes snapshot from VM" {
			$fakeVM | Remove-VMSnapshot -Name "MySnapshot"
			$script:TestFlag | should be $true
		}
	}
}