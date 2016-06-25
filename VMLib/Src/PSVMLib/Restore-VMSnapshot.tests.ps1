.  $PSScriptRoot\Restore-VMSnapshot.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Restore-VMSnapshot" {
	Context "Can restore VM snapshot" {
		$script:TestFlag = $false	
		$fakevm = New-FakeVM -RestoreSnapshot {
			param($Name)

			if($name -eq "MySnapshot") { $script:TestFlag = $true}

		}
	
		It "Can restor snapshot" {
			$fakevm | Restore-VMSnapshot -Name "MySnapshot"
			$script:TestFlag | should be $true
		}
	}
}