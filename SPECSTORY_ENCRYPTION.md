# Git-Crypt Setup for .specstory Folder

## ✅ Setup Complete!

Your `.specstory` folder is now encrypted with git-crypt. Here's what this means:

### What Happened:
- ✅ Git-crypt was initialized in this repository
- ✅ `.gitattributes` was created to encrypt `.specstory/**` files
- ✅ Your `.specstory` folder was committed with encryption
- ✅ Encryption key exported to `../specstory-encryption-key.key`

### How It Works:
- **For you**: You see and edit `.specstory` files normally (they're decrypted automatically)
- **For others**: They see only encrypted binary blobs in the repository
- **In git**: The files are stored encrypted, but you work with them decrypted

### Important Files:
- `../specstory-encryption-key.key` - **KEEP THIS SAFE AND PRIVATE!**
- `.gitattributes` - Tells git which files to encrypt

### Usage:

#### On This Machine:
- Just work normally with `.specstory` files
- Git automatically encrypts them when you commit
- Git automatically decrypts them when you checkout

#### On Another Machine:
```bash
# Clone the repository
git clone <your-repo>
cd <repo>

# Unlock with your key
git-crypt unlock /path/to/specstory-encryption-key.key

# Now you can see decrypted .specstory files
```

#### To Add New Team Members (if needed):
```bash
# Add their GPG key to git-crypt
git-crypt add-gpg-user USER_ID

# Or share the symmetric key file (less secure)
```

### Security Notes:
- ⚠️ **Never commit the `.key` file to git**
- ⚠️ **Keep your encryption key backed up safely**
- ✅ Others cannot read your conversations without the key
- ✅ You can work with files normally while they remain encrypted in git

### Testing:
To verify encryption is working:
```bash
# Check what others see (should be encrypted binary)
git show HEAD:.specstory/.what-is-this.md

# Check what you see (should be readable text)
cat .specstory/.what-is-this.md
```

### Recovery:
If you lose access and need to decrypt on a new machine:
1. Clone the repository
2. Run: `git-crypt unlock /path/to/specstory-encryption-key.key`
3. Your `.specstory` files will be readable again

Your conversations are now safely encrypted in git! 🔒