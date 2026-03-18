.  $PSScriptRoot\Test-ItemInVM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1

Describe "Test-ItemInVM" {
	Context "Tests if item exsts in vm." {
		$fakevm = New-FakeVM -FileExists {
			param($path)

			if($path -eq "c:\myfile.txt") { return $true}

			return $false

		} -DirectoryExists {
			param($path)

			if($path -eq "c:\myfolder") { return $true}

			return $false
		}

		It "Test if file exists in vm." {
			$fakevm | Test-ItemInVM -Path "c:\myfile.txt" -Type File | should be $true
		}

		It "Test if folder exists in vm." {
			$fakevm | Test-ItemInVM -Path "c:\myfolder" -Type Directory | should be $true
		}

	}
}