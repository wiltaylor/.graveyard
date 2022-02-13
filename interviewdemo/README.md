# Interview Demo Application
This is a simple application designed to meet the specifications supplied and
show the use of the following technologies:

- .NET Core 6 ASP.NET
- XUnit
- MOQ
- SignalR
- Angular and Typescript
- Jasmin Unit Test framework
- Docker

## Build Instructions
### IDE (Visual Studio 2022/Rider 2021)
Simply open the project and run it.

### Docker
You can build and run the project in docker by doing the following:

- cd into the main directory of the project
- Run the following:
```shell
docker build . -t wiltaylor/demo
```
- Now to run the project run:
```shell
docker run -p 8080:80 wiltaylor/demo
```

## License (MIT)

Copyright (c) 2022 Wil Taylor

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.