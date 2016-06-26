.  $PSScriptRoot\New-VMHardware.ps1
.  $PSScriptRoot\Fakes\Fake_Hardware.ps1

Describe "New-VMHardware" {
	Context "Can create VM Hardware" {
		
	
		It "Can create a network card" {		
			New-VMHardware -Network | should not be $null
		}

		It "Can create a floppy drive" {
			New-VMHardware -Disk Floppy | should not be $null
		}

	    It "Can create a cdrom drive" {
			New-VMHardware -Disk CDRom | should not be $null
		}

	    It "Can create a hard drive" {
			New-VMHardware -Disk HardDisk | should not be $null
		}
	}
}