# 极速文件分发系统 Filer Version 1.0.6
### 传统文件复制需要20分钟，Filer只需20秒。
### Filer能充分利用电脑系统的性能，将多核平行处理能力用足，将网络带宽跑满。
			
## Filer是如何做到的：
- 多路复用，并行传送。小文件多个文件一起传送，大文件分成多个片段同时传送。 
- 文件对比，只传不同。计算文件内容哈希值（Blake3 Hasher，目前速度最快的安全算法），用服务器端与本地比对，只传送不同的文件。 
- 重复检查，一次下载。相同文件仅从服务器下载一次，其他位置文件从本地复制。

## Filer如何用：
filer客户端与服务器端集成在一个可执行文件中。通过命令行参数与系统配置文件filer.json来工作。
下面以如何将服务器端.\demo_sent文件夹下所有文件传送到客户端.\demo_recv为例来说明用法：
将filer.exe放在当前文件夹下，在当前文件夹下建立public子文件夹，将index.html放在public下。
在当前文件夹下建立两个子文件夹，demo_sent用于发送文件，demo_recv用于接收文件。

### 启动服务端
1. 修改filer.json，配置http与https有关参数，以及demo这个分支（catalog）中的内容:
```
{
    "server": {
        "static_path": "./public",
        "server_name": "Filer",
        "http_active": true,
        "http_port": 9191,
        "https_active": false,
        "https_port": 443,
        "https_cert": "server.cer",
        "https_key": "server.key"
    },
    "demo":{
        "path": "./demo_sent",
        "part_size": 1024000,
        "max_tasks": 32,
        "list_name": "filelist.txt"
    }
}
```   

3. 启动filer.exe扫描.\demo_sent文件夹下的所有文件，计算哈希值，写入文件目录.\demo_sent\filelist.txt中，每次服务器端文件更新，都需要通过这更步骤来更新服务器端文件袋哈希值，写入filelist.txt中。   
```
   filer -i -c demo
```   

4. 启动文件服务
```
   filer -s
```   

### 检查服务器端是否正常启动
```
打开浏览器，地址栏输入服务器ip地址:9191, 本机输入: http://127.0.0.1:9191, 如果能够显示index.html中的内容，服务启动正常，否则检查配置文件filer.json与服务器防火墙设置。
```
### 启动客户端
1. 修改filer.json，配置其中的client分支中的内容:
```
{
    "client": {
        "server": "127.0.0.1",
        "port": 9191,
        "is_https": false,
        "catalog": "demo",
        "path": "./demo_recv",
        "max_tasks": 128,
        "kill_running_exe": true
    },
}    
```   
2. 下载文件
```
  filer -d 下载服务器上的所有文件
  filer -u 通过将本地filelist.txt中的内容与远程filelist.txt中的内容做比较，下载服务器上的已经更新的文件覆盖本地文件，同时用服务器端的filelist.txt覆盖本地filelist.txt文件。
``` 