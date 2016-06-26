.  $PSScriptRoot\New-VM.ps1
.  $PSScriptRoot\Fakes\Fake_VM.ps1
.  $PSScriptroot\Fakes\Fake_Hypervisor.ps1

Describe "New-VM" {
	Context "Can create new VM" {
		$FakeHypervisor = New-FakeHypervisor -CreateNewVM {
			param($info)

			if($info.Properties.ContainsKey("MySetting")) {
				return New-FakeVM
			}

			throw "Not expected object"
		}

		$info = New-Object PSObject
		$info | Add-Member -MemberType NoteProperty -Name PRoperties -Value (New-Object 'system.collections.generic.dictionary[string,string]')
		$info.Properties.Add("MySetting", "MyValue")
		

		It "Can create vm with hypervisor object" {
			$FakeHypervisor | New-VM -Name "MyVM" -Path "c:\mynewvm.vmx" -Info $info | should not be $null
		}
	}
}