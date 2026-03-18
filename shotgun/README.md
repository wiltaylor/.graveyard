# Shotgun
This cmdlet lets the user trigger a script block repeatedly by simply pressing enter.
This is useful for development work flows where you need to run the same commands again and again.

## How to install
To install run the following command and agree to any prompts

Install-Module shotgun

## How to use

There are two ways you can use Shotgun. First way is to pass a block of code you wish to run multiple times. This will just run the
command multiple times till the user quits with ctrl + c or q.

```
shotgun { Write-Host "Foo" }
```

The second method is to use a shotgun.psd1 file. This allows you to specify multiple commands that can be run from shotgun.
In the shotgun.psd1 file you just simply create a dictionary with the script blocks as properties and shotgun will find them.

```
shotgun
```

An example file layout is as follows:

```
    @{
        Action1 = {
            Write-Host "Action 1 ran"
        }

        Action2 = {
            Write-Host "Action 2 ran"
        }

        Action3 = {
            Write-Host "Action 3 ran"
        }
    } 
```
## License
Copyright 2017 Wil Taylor

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
